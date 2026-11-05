// SPDX-License-Identifier: Apache-2.0
//! Compound File Binary image builder for the crate's `#[cfg(test)]` suites.
//!
//! The builder writes a version 3 image from a tree of storages and streams.
//! Streams below the 4096-byte cutoff go to the mini stream, larger streams to
//! regular sectors. The directory sibling tree is a right-leaning chain in
//! the CFB name order, all black, which the format admits.
#![allow(clippy::unwrap_used)]

use cadmpeg_test_support::bytes::{put_u16, put_u32};

const SECTOR: usize = 512;
const MINI_SECTOR: usize = 64;
const MINI_CUTOFF: usize = 4096;
const FREE: u32 = 0xffff_ffff;
const END: u32 = 0xffff_fffe;
const FAT: u32 = 0xffff_fffd;
const ENTRIES_PER_FAT_SECTOR: usize = SECTOR / 4;

/// One directory member: a storage with children or a stream with bytes.
pub(crate) enum Node {
    Storage(Vec<(String, Node)>),
    Stream(Vec<u8>),
}

/// A named stream.
pub(crate) fn stream(name: &str, bytes: &[u8]) -> (String, Node) {
    (name.to_owned(), Node::Stream(bytes.to_vec()))
}

/// A named storage.
pub(crate) fn storage(name: &str, children: Vec<(String, Node)>) -> (String, Node) {
    (name.to_owned(), Node::Storage(children))
}

struct Entry {
    name: String,
    object_type: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

fn name_key(name: &str) -> (usize, Vec<u16>) {
    let units: Vec<u16> = name.encode_utf16().collect();
    let upper = name.to_uppercase().encode_utf16().collect();
    (units.len(), upper)
}

fn id(index: usize) -> u32 {
    u32::try_from(index).unwrap()
}

/// Flatten siblings into `entries`; return the id of the first sibling.
fn flatten(
    mut children: Vec<(String, Node)>,
    entries: &mut Vec<Entry>,
    streams: &mut Vec<(usize, Vec<u8>)>,
) -> u32 {
    if children.is_empty() {
        return FREE;
    }
    children.sort_by_key(|(name, _)| name_key(name));
    let first = entries.len();
    let count = children.len();
    for (offset, (name, node)) in children.iter().enumerate() {
        entries.push(Entry {
            name: name.clone(),
            object_type: match node {
                Node::Storage(_) => 1,
                Node::Stream(_) => 2,
            },
            left: FREE,
            right: if offset + 1 < count {
                id(first + offset + 1)
            } else {
                FREE
            },
            child: FREE,
            start: END,
            size: 0,
        });
    }
    for (offset, (_, node)) in children.into_iter().enumerate() {
        let index = first + offset;
        match node {
            Node::Storage(inner) => {
                let child = flatten(inner, entries, streams);
                entries[index].child = child;
            }
            Node::Stream(bytes) => {
                entries[index].size = u64::try_from(bytes.len()).unwrap();
                streams.push((index, bytes));
            }
        }
    }
    id(first)
}

fn sectors_for(bytes: usize, size: usize) -> usize {
    bytes.div_ceil(size)
}

/// Build a Compound File image whose root storage holds `children`.
pub(crate) fn compound_file(children: Vec<(String, Node)>) -> Vec<u8> {
    let mut entries = vec![Entry {
        name: "Root Entry".to_owned(),
        object_type: 5,
        left: FREE,
        right: FREE,
        child: FREE,
        start: END,
        size: 0,
    }];
    let mut streams = Vec::new();
    entries[0].child = flatten(children, &mut entries, &mut streams);

    let directory_sectors = sectors_for(entries.len(), SECTOR / 128);
    let mut mini_stream = Vec::new();
    let mut mini_fat: Vec<u32> = Vec::new();
    let mut regular: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut mini_starts = Vec::new();
    for (index, bytes) in streams {
        if bytes.is_empty() {
            continue;
        }
        if bytes.len() < MINI_CUTOFF {
            let first = mini_fat.len();
            let count = sectors_for(bytes.len(), MINI_SECTOR);
            for position in 0..count {
                mini_fat.push(if position + 1 == count {
                    END
                } else {
                    id(first + position + 1)
                });
            }
            mini_stream.extend_from_slice(&bytes);
            mini_stream.resize(mini_fat.len() * MINI_SECTOR, 0);
            mini_starts.push((index, id(first)));
        } else {
            regular.push((index, bytes));
        }
    }
    let mini_fat_sectors = sectors_for(mini_fat.len(), ENTRIES_PER_FAT_SECTOR);
    let mini_stream_sectors = sectors_for(mini_stream.len(), SECTOR);
    let regular_sectors: usize = regular
        .iter()
        .map(|(_, bytes)| sectors_for(bytes.len(), SECTOR))
        .sum();
    let body = directory_sectors + mini_fat_sectors + mini_stream_sectors + regular_sectors;
    let mut fat_sectors = 1;
    while fat_sectors * ENTRIES_PER_FAT_SECTOR < body + fat_sectors {
        fat_sectors += 1;
    }
    assert!(fat_sectors <= 109, "builder writes no DIFAT chain");

    let total = body + fat_sectors;
    let mut file = vec![0_u8; SECTOR * (1 + total)];
    let mut fat = vec![FREE; fat_sectors * ENTRIES_PER_FAT_SECTOR];
    let chain = |fat: &mut [u32], first: usize, count: usize| {
        for position in 0..count {
            fat[first + position] = if position + 1 == count {
                END
            } else {
                id(first + position + 1)
            };
        }
    };

    let directory_first = 0;
    chain(&mut fat, directory_first, directory_sectors);
    let mini_fat_first = directory_sectors;
    if mini_fat_sectors > 0 {
        chain(&mut fat, mini_fat_first, mini_fat_sectors);
    }
    let mini_stream_first = mini_fat_first + mini_fat_sectors;
    if mini_stream_sectors > 0 {
        chain(&mut fat, mini_stream_first, mini_stream_sectors);
        entries[0].start = id(mini_stream_first);
        entries[0].size = u64::try_from(mini_stream.len()).unwrap();
    }
    for (index, start) in mini_starts {
        entries[index].start = start;
    }
    let mut next = mini_stream_first + mini_stream_sectors;
    for (index, bytes) in &regular {
        let count = sectors_for(bytes.len(), SECTOR);
        chain(&mut fat, next, count);
        entries[*index].start = id(next);
        let at = (1 + next) * SECTOR;
        file[at..at + bytes.len()].copy_from_slice(bytes);
        next += count;
    }
    let fat_first = next;
    for sector in 0..fat_sectors {
        fat[fat_first + sector] = FAT;
    }

    file[..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
    put_u16(&mut file, 24, 0x003e);
    put_u16(&mut file, 26, 3);
    put_u16(&mut file, 28, 0xfffe);
    put_u16(&mut file, 30, 9);
    put_u16(&mut file, 32, 6);
    put_u32(&mut file, 44, id(fat_sectors));
    put_u32(&mut file, 48, id(directory_first));
    put_u32(&mut file, 56, u32::try_from(MINI_CUTOFF).unwrap());
    put_u32(
        &mut file,
        60,
        if mini_fat_sectors > 0 {
            id(mini_fat_first)
        } else {
            END
        },
    );
    put_u32(&mut file, 64, id(mini_fat_sectors));
    put_u32(&mut file, 68, END);
    put_u32(&mut file, 72, 0);
    for slot in 0..109 {
        put_u32(
            &mut file,
            76 + slot * 4,
            if slot < fat_sectors {
                id(fat_first + slot)
            } else {
                FREE
            },
        );
    }

    let mut directory = vec![0_u8; directory_sectors * SECTOR];
    for slot in directory.chunks_exact_mut(128) {
        slot[68..80].fill(0xff);
    }
    for (index, entry) in entries.iter().enumerate() {
        let slot = &mut directory[index * 128..(index + 1) * 128];
        let units: Vec<u16> = entry.name.encode_utf16().collect();
        for (position, unit) in units.iter().enumerate() {
            put_u16(slot, position * 2, *unit);
        }
        put_u16(slot, 64, u16::try_from((units.len() + 1) * 2).unwrap());
        slot[66] = entry.object_type;
        slot[67] = 1;
        put_u32(slot, 68, entry.left);
        put_u32(slot, 72, entry.right);
        put_u32(slot, 76, entry.child);
        put_u32(slot, 116, entry.start);
        slot[120..128].copy_from_slice(&entry.size.to_le_bytes());
    }
    let at = SECTOR * (1 + directory_first);
    file[at..at + directory.len()].copy_from_slice(&directory);

    let at = SECTOR * (1 + mini_fat_first);
    for (position, value) in mini_fat.iter().enumerate() {
        put_u32(&mut file, at + position * 4, *value);
    }
    if mini_fat_sectors > 0 {
        let padded = mini_fat_sectors * ENTRIES_PER_FAT_SECTOR;
        for position in mini_fat.len()..padded {
            put_u32(&mut file, at + position * 4, FREE);
        }
    }
    let at = SECTOR * (1 + mini_stream_first);
    file[at..at + mini_stream.len()].copy_from_slice(&mini_stream);
    for (position, value) in fat.iter().enumerate() {
        put_u32(&mut file, SECTOR * (1 + fat_first) + position * 4, *value);
    }
    file
}
