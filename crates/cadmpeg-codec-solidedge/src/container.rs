// SPDX-License-Identifier: Apache-2.0
//! Locate the Parasolid streams of a Solid Edge Compound File.
//!
//! The Parasolid data of a document sits in the `PARASOLID` storage. A leaf
//! name is a stem and an extension. The extension `P_B` marks a partition
//! transmit stream, `D_B` a deltas transmit stream, and `X_B` a standalone
//! transmit stream. A stem names the body group the stream belongs to: the
//! streams `STREAM0.P_B` and `STREAM0.D_B` form the group `STREAM0`. The stems
//! `SKETCH` and `SVSKETCH` mark sketch streams, which hold no body. [`scan`]
//! reads the directory and classifies the entries of that storage. It opens a
//! stream only when [`Scan::open`] asks.

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

/// Root-level members that identify a Solid Edge document. `PARASOLID` holds
/// the geometry. `DocVersion2` and `PSMroots` mark a model document. `FamilyInfo`
/// marks a family-of-parts master.
const ROOT_MARKERS: [&str; 4] = [PARASOLID_STORAGE, "DocVersion2", "PSMroots", "FamilyInfo"];

/// Root-level member that marks a family-of-parts master.
const FAMILY_MARKER: &str = "FamilyInfo";

/// Stems of the sketch streams.
const SKETCH_STEMS: [&str; 2] = ["SKETCH", "SVSKETCH"];

/// What a stream of the `PARASOLID` storage holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamRole {
    /// A partition transmit stream, `<stem>.P_B`.
    Partition,
    /// A deltas transmit stream, `<stem>.D_B`.
    Deltas,
    /// A standalone transmit stream, `<stem>.X_B`.
    Transmit,
    /// A sketch stream, `SKETCH.P_B` or `SVSKETCH.P_B`.
    Sketch,
}

impl StreamRole {
    /// The label that inspection reports.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Partition => "partition",
            Self::Deltas => "deltas",
            Self::Transmit => "transmit",
            Self::Sketch => "sketch",
        }
    }
}

/// A leaf name that follows the `<stem>.P_B`, `<stem>.D_B`, or `<stem>.X_B`
/// pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StreamName {
    /// The stem in ASCII upper case. It names the body group.
    pub(crate) group: String,
    /// The role that the extension and the stem state.
    pub(crate) role: StreamRole,
}

impl StreamName {
    /// Parse a leaf name, comparing the fixed parts without regard to ASCII case.
    pub(crate) fn parse(leaf: &str) -> Option<Self> {
        let (stem, extension) = leaf.rsplit_once('.')?;
        if stem.is_empty() {
            return None;
        }
        let group = stem.to_ascii_uppercase();
        let by_extension = if extension.eq_ignore_ascii_case("P_B") {
            StreamRole::Partition
        } else if extension.eq_ignore_ascii_case("D_B") {
            StreamRole::Deltas
        } else if extension.eq_ignore_ascii_case("X_B") {
            StreamRole::Transmit
        } else {
            return None;
        };
        let role = if SKETCH_STEMS.contains(&group.as_str()) {
            StreamRole::Sketch
        } else {
            by_extension
        };
        Some(Self { group, role })
    }
}

/// The decimal index of an upper-case `STREAM<n>` stem.
fn group_number(group: &str) -> Option<u32> {
    let digits = group.strip_prefix("STREAM")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// One stream below the `PARASOLID` storage.
#[derive(Debug, Clone)]
pub(crate) struct ParasolidEntry {
    /// Directory path, as the file spells it.
    pub(crate) path: String,
    /// The parsed name, when the leaf follows the stream pattern.
    pub(crate) name: Option<StreamName>,
}

/// The body streams that share one stem.
#[derive(Debug)]
pub(crate) struct BodyGroup<'s> {
    /// The stem in ASCII upper case.
    pub(crate) key: &'s str,
    /// The `P_B` stream.
    pub(crate) partition: Option<&'s ParasolidEntry>,
    /// The `D_B` stream.
    pub(crate) deltas: Option<&'s ParasolidEntry>,
    /// The `X_B` stream.
    pub(crate) transmit: Option<&'s ParasolidEntry>,
}

impl<'s> BodyGroup<'s> {
    /// The stream that holds the base state, with its role. A partition stream
    /// wins over a standalone transmit stream.
    pub(crate) fn base(&self) -> Option<(&'s ParasolidEntry, StreamRole)> {
        self.partition
            .map(|entry| (entry, StreamRole::Partition))
            .or_else(|| self.transmit.map(|entry| (entry, StreamRole::Transmit)))
    }
}

/// What the root-level members say a Compound File is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentKind {
    /// A document with a `PARASOLID` storage.
    Model,
    /// A family-of-parts master, which holds no model.
    FamilyMaster,
    /// A document with model markers and no `PARASOLID` storage.
    ModelWithoutGeometry,
    /// No Solid Edge marker.
    Unrecognized,
}

/// The parsed directory of one Compound File.
pub(crate) struct Scan<'a> {
    snapshot: CompoundSnapshot<'a>,
    storage: Option<String>,
    entries: Vec<ParasolidEntry>,
    kind: DocumentKind,
}

impl<'a> Scan<'a> {
    /// The `PARASOLID` storage path as the directory spells it.
    pub(crate) fn storage(&self) -> Option<&str> {
        self.storage.as_deref()
    }

    /// What the root-level members say the document is.
    pub(crate) fn kind(&self) -> DocumentKind {
        self.kind
    }

    /// Every stream below the `PARASOLID` storage, in directory order.
    pub(crate) fn entries(&self) -> &[ParasolidEntry] {
        &self.entries
    }

    /// The body groups, ordered by `STREAM<n>` index first and by stem after.
    pub(crate) fn body_groups(
        &self,
        ctx: &DecodeContext<'_>,
    ) -> Result<Vec<BodyGroup<'_>>, CodecError> {
        ctx.charge_work(
            u64_from_index(self.entries.len()),
            "group Solid Edge body streams",
        )?;
        let mut groups: Vec<BodyGroup<'_>> = Vec::new();
        for entry in &self.entries {
            let Some(name) = &entry.name else { continue };
            if name.role == StreamRole::Sketch {
                continue;
            }
            let position = match groups.iter().position(|group| group.key == name.group) {
                Some(position) => position,
                None => {
                    ctx.push_vec(
                        &mut groups,
                        BodyGroup {
                            key: &name.group,
                            partition: None,
                            deltas: None,
                            transmit: None,
                        },
                        "collect Solid Edge body groups",
                    )?;
                    groups.len() - 1
                }
            };
            let group = &mut groups[position];
            let slot = match name.role {
                StreamRole::Partition => &mut group.partition,
                StreamRole::Deltas => &mut group.deltas,
                StreamRole::Transmit => &mut group.transmit,
                StreamRole::Sketch => continue,
            };
            *slot = Some(entry);
        }
        ctx.stable_sort_by(
            &mut groups,
            |left, right| {
                match (group_number(left.key), group_number(right.key)) {
                    (Some(left), Some(right)) => left.cmp(&right),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
                .then_with(|| left.key.cmp(right.key))
            },
            |group| group.key.len(),
            "order Solid Edge body groups",
        )?;
        Ok(groups)
    }

    /// The sketch streams, in directory order.
    pub(crate) fn sketches(&self) -> impl Iterator<Item = &ParasolidEntry> {
        self.entries.iter().filter(|entry| {
            entry
                .name
                .as_ref()
                .is_some_and(|name| name.role == StreamRole::Sketch)
        })
    }

    /// The directory stream behind an entry.
    pub(crate) fn stream(&self, entry: &ParasolidEntry) -> Option<&CompoundStreamEntry> {
        self.snapshot.stream(&entry.path)
    }

    /// Open one root-level stream by name, when the directory has it.
    pub(crate) fn open_root_stream(
        &self,
        ctx: &DecodeContext<'a>,
        name: &str,
    ) -> Result<Option<View<'a>>, CodecError> {
        match self.snapshot.stream(name) {
            Some(stream) => Ok(Some(self.snapshot.open(ctx, stream)?)),
            None => Ok(None),
        }
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

/// Whether a directory path names a root-level Solid Edge marker or a member
/// of the marker storage.
fn is_marker_path(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or(path);
    ROOT_MARKERS
        .iter()
        .any(|marker| first.eq_ignore_ascii_case(marker))
}

/// Classify the document by the first component of every directory path.
fn kind_of<'p>(paths: impl Iterator<Item = &'p str>) -> DocumentKind {
    let (mut parasolid, mut family, mut model) = (false, false, false);
    for path in paths {
        let first = path.split('/').next().unwrap_or(path);
        if first.eq_ignore_ascii_case(PARASOLID_STORAGE) {
            parasolid = true;
        } else if first.eq_ignore_ascii_case(FAMILY_MARKER) {
            family = true;
        } else if ROOT_MARKERS
            .iter()
            .any(|marker| first.eq_ignore_ascii_case(marker))
        {
            model = true;
        }
    }
    if parasolid {
        DocumentKind::Model
    } else if model {
        DocumentKind::ModelWithoutGeometry
    } else if family {
        DocumentKind::FamilyMaster
    } else {
        DocumentKind::Unrecognized
    }
}

/// What a detection prefix shows of a Solid Edge document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Evidence {
    /// A Compound File with a Solid Edge root marker.
    Marker,
    /// A Compound File whose directory is structurally invalid. It can be a
    /// damaged Solid Edge document, so the decode reports why it cannot read it.
    Unreadable,
    /// No Solid Edge evidence.
    None,
}

/// Classify a detection prefix.
pub(crate) fn looks_like_solidedge(
    ctx: &DecodeContext<'_>,
    prefix: &[u8],
) -> Result<Evidence, CodecError> {
    if !prefix.starts_with(&COMPOUND_FILE_MAGIC) {
        return Ok(Evidence::None);
    }
    let (probe, _storage) =
        CompoundPrefixProbe::inspect_with_context(ctx, View::over_retained(prefix))?;
    if matches!(probe, CompoundPrefixProbe::Malformed(_)) {
        return Ok(Evidence::Unreadable);
    }
    let Some(paths) = probe.paths() else {
        return Ok(Evidence::None);
    };
    for path in paths {
        ctx.charge_work(
            u64_from_index(path.len()),
            "compare Solid Edge directory evidence",
        )?;
        if is_marker_path(path) {
            return Ok(Evidence::Marker);
        }
    }
    Ok(Evidence::None)
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
    let kind = kind_of(snapshot.entries().iter().map(CompoundEntry::path));
    Ok(Scan {
        snapshot,
        storage,
        entries,
        kind,
    })
}

#[cfg(test)]
mod tests;
