// SPDX-License-Identifier: Apache-2.0
//! Locate the Parasolid streams of a Solid Edge Compound File.
//!
//! The Parasolid body of a document sits in the `PARASOLID` storage. A stream
//! named `STREAM<n>.P_B` holds a partition transmit stream and a stream named
//! `STREAM<n>.D_B` holds a deltas transmit stream. [`scan`] reads the
//! directory and classifies the entries of that storage. It opens a stream only
//! when [`Scan::open`] asks.

use cadmpeg_container::compound::{
    CompoundEntry, CompoundPrefixProbe, CompoundSnapshot, CompoundStreamEntry,
};
use cadmpeg_core::container::ContainerRole;
use cadmpeg_core::decode::{u64_from_index, DecodeContext, View};
use cadmpeg_core::{CodecError, ContainerEntry};

/// Leading signature of a Compound File.
const COMPOUND_FILE_MAGIC: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

/// Name of the storage that holds the Parasolid streams.
pub(crate) const PARASOLID_STORAGE: &str = "PARASOLID";

/// Index of the stream pair that decode reads.
pub(crate) const BODY_STREAM_INDEX: u32 = 0;

/// What a stream of the `PARASOLID` storage holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamRole {
    /// A partition transmit stream, `STREAM<n>.P_B`.
    Partition,
    /// A deltas transmit stream, `STREAM<n>.D_B`.
    Deltas,
}

impl StreamRole {
    /// The label that inspection reports.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Partition => "partition",
            Self::Deltas => "deltas",
        }
    }
}

/// A stream name that follows the `STREAM<n>.P_B` or `STREAM<n>.D_B` pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StreamName {
    /// The decimal index `n`.
    pub(crate) index: u32,
    /// The role that the extension states.
    pub(crate) role: StreamRole,
}

impl StreamName {
    /// Parse a leaf name, comparing the fixed parts without regard to ASCII case.
    pub(crate) fn parse(leaf: &str) -> Option<Self> {
        let (stem, extension) = leaf.rsplit_once('.')?;
        let role = if extension.eq_ignore_ascii_case("P_B") {
            StreamRole::Partition
        } else if extension.eq_ignore_ascii_case("D_B") {
            StreamRole::Deltas
        } else {
            return None;
        };
        let digits = stem
            .get(..6)
            .filter(|head| head.eq_ignore_ascii_case("STREAM"))?;
        let digits = stem.get(digits.len()..)?;
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        Some(Self {
            index: digits.parse().ok()?,
            role,
        })
    }
}

/// One stream below the `PARASOLID` storage.
#[derive(Debug, Clone)]
pub(crate) struct ParasolidEntry {
    /// Directory path, as the file spells it.
    pub(crate) path: String,
    /// The parsed name, when the leaf follows the stream pattern.
    pub(crate) name: Option<StreamName>,
}

/// The parsed directory of one Compound File.
pub(crate) struct Scan<'a> {
    snapshot: CompoundSnapshot<'a>,
    storage: Option<String>,
    entries: Vec<ParasolidEntry>,
}

impl<'a> Scan<'a> {
    /// The `PARASOLID` storage path as the directory spells it.
    pub(crate) fn storage(&self) -> Option<&str> {
        self.storage.as_deref()
    }

    /// Every stream below the `PARASOLID` storage, in directory order.
    pub(crate) fn entries(&self) -> &[ParasolidEntry] {
        &self.entries
    }

    /// The entry that holds one stream of the decoded pair.
    pub(crate) fn body_stream(&self, role: StreamRole) -> Option<&ParasolidEntry> {
        self.entries.iter().find(|entry| {
            entry
                .name
                .is_some_and(|name| name.index == BODY_STREAM_INDEX && name.role == role)
        })
    }

    /// The directory stream behind an entry.
    pub(crate) fn stream(&self, entry: &ParasolidEntry) -> Option<&CompoundStreamEntry> {
        self.snapshot.stream(&entry.path)
    }

    /// Open one entry as a view of its logical bytes.
    pub(crate) fn open(
        &self,
        ctx: &DecodeContext<'a>,
        entry: &ParasolidEntry,
    ) -> Result<View<'a>, CodecError> {
        let stream = self.stream(entry).ok_or_else(|| {
            CodecError::malformed(format_args!(
                "Solid Edge directory has no stream {}",
                entry.path
            ))
        })?;
        self.snapshot.open(ctx, stream)
    }

    /// The container summary entries of the whole directory.
    pub(crate) fn container_entries(
        &self,
        ctx: &DecodeContext<'_>,
    ) -> Result<Vec<ContainerEntry>, CodecError> {
        ctx.charge_work(
            u64_from_index(self.snapshot.entries().len()),
            "classify Solid Edge directory entries",
        )?;
        let mut entries = self.snapshot.container_entries(|entry| match entry {
            CompoundEntry::Storage(_) => ContainerRole::Storage,
            CompoundEntry::Stream(stream) => {
                if self.parasolid_name(stream.path()).is_some() {
                    ContainerRole::ParasolidStream
                } else {
                    ContainerRole::CompoundStream
                }
            }
        });
        for entry in &mut entries {
            if let Some(name) = self.parasolid_name(&entry.name) {
                let key =
                    ctx.copy_retained_text("parasolid_role", "retain Solid Edge entry key")?;
                let value = ctx.copy_retained_text(name.role.label(), "retain Solid Edge role")?;
                ctx.insert_btree_map(
                    &mut entry.attributes,
                    key,
                    value,
                    "collect Solid Edge entry attributes",
                )?;
            }
        }
        Ok(entries)
    }

    /// The parsed stream name of a path directly below the storage.
    fn parasolid_name(&self, path: &str) -> Option<StreamName> {
        let storage = self.storage.as_deref()?;
        let leaf = path_below(path, storage)?;
        StreamName::parse(leaf)
    }
}

/// The remainder of `path` below `storage`, or `None` when `path` is outside it
/// or below a nested storage.
fn path_below<'p>(path: &'p str, storage: &str) -> Option<&'p str> {
    let head = path.get(..storage.len())?;
    if !head.eq_ignore_ascii_case(storage) {
        return None;
    }
    let rest = path.get(storage.len()..)?.strip_prefix('/')?;
    (!rest.contains('/')).then_some(rest)
}

/// Whether a directory path names the `PARASOLID` storage or a member of it.
fn is_parasolid_path(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or(path);
    first.eq_ignore_ascii_case(PARASOLID_STORAGE)
}

/// Whether a detection prefix is a Compound File with a `PARASOLID` storage.
pub(crate) fn looks_like_solidedge(
    ctx: &DecodeContext<'_>,
    prefix: &[u8],
) -> Result<bool, CodecError> {
    if !prefix.starts_with(&COMPOUND_FILE_MAGIC) {
        return Ok(false);
    }
    let (probe, _storage) =
        CompoundPrefixProbe::inspect_with_context(ctx, View::over_retained(prefix))?;
    let Some(paths) = probe.paths() else {
        return Ok(false);
    };
    for path in paths {
        ctx.charge_work(
            u64_from_index(path.len()),
            "compare Solid Edge directory evidence",
        )?;
        if is_parasolid_path(path) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Read the directory of a Compound File and classify the `PARASOLID` entries.
pub(crate) fn scan<'a>(ctx: &DecodeContext<'a>, root: View<'a>) -> Result<Scan<'a>, CodecError> {
    if !root.window().starts_with(&COMPOUND_FILE_MAGIC) {
        return Err(CodecError::WrongFormat(
            "not a Solid Edge document: no Compound File signature".into(),
        ));
    }
    let snapshot = CompoundSnapshot::new(ctx, root)?;
    ctx.charge_work(
        u64_from_index(snapshot.entries().len()),
        "scan Solid Edge directory",
    )?;
    let mut storage = None;
    for entry in snapshot.entries() {
        if let CompoundEntry::Storage(found) = entry {
            if found.path().eq_ignore_ascii_case(PARASOLID_STORAGE) {
                storage = Some(ctx.copy_retained_text(found.path(), "retain Solid Edge storage")?);
                break;
            }
        }
    }
    let mut entries = Vec::new();
    if let Some(storage) = storage.as_deref() {
        for entry in snapshot.entries() {
            let CompoundEntry::Stream(stream) = entry else {
                continue;
            };
            let Some(head) = stream.path().get(..storage.len()) else {
                continue;
            };
            if !head.eq_ignore_ascii_case(storage)
                || !stream.path()[storage.len()..].starts_with('/')
            {
                continue;
            }
            let name = path_below(stream.path(), storage).and_then(StreamName::parse);
            let path = ctx.copy_retained_text(stream.path(), "retain Solid Edge stream path")?;
            ctx.push_vec(
                &mut entries,
                ParasolidEntry { path, name },
                "collect Solid Edge Parasolid entries",
            )?;
        }
    }
    Ok(Scan {
        snapshot,
        storage,
        entries,
    })
}

#[cfg(test)]
mod tests;
