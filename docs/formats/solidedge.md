# Siemens Solid Edge `.par`, `.psm`, and `.asm` (CFB + Parasolid): Format Specification

> **License:** This document is released under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/). Attribute to the cadmpeg project.

---

## 1. Format overview

A Solid Edge part (`.par`), sheet-metal part (`.psm`), or assembly (`.asm`) is a Compound File Binary (CFB) document as defined by MS-CFB. The B-rep of the document is a Parasolid partition stream and a Parasolid deltas stream, or one standalone Parasolid transmit stream. The streams are members of one storage named `PARASOLID`.

**Byte order and units (global):**

- CFB header, FAT, directory, and sector fields are **little-endian**.
- Parasolid binary transmit fields are **big-endian**.
- Parasolid geometric doubles are in **metres**. Model geometry in the neutral model is millimetres (×1000).

The file extension is not a format discriminant. A CFB document is a Solid Edge document with a Parasolid body exactly when its directory has a `PARASOLID` storage (section 2.2).

---

## 2. Container

### 2.1 Compound File

The container follows MS-CFB. Stream and storage names are compared without regard to case, by the MS-CFB name ordering: names compare by UTF-16 length first and then by uppercased code units. A stream shorter than the mini-stream cutoff is stored in the mini stream and is read through the mini-FAT. Any other stream is stored in regular sectors.

MS-CFB 2.6.1 forbids the characters `/`, `\`, `:`, and `!` in a directory name. Documents exist whose directories hold names with `:`, for example `S:Master`.

The reader accepts `:`, `!`, and `\` as ordinary characters in a name. It refuses `/` because `/` separates the components of a path.

The reader tolerates two deviations from MS-CFB that readers in common use accept. The first deviation is a directory sibling tree that breaks the red-black colour rules, for example two red nodes that are linked. The reader ignores the colour of the nodes and keeps the check of the name order, the links, and the cycles. The second deviation is a header that counts more FAT sectors or DIFAT sectors than the DIFAT chain lists. The reader uses the FAT sectors that the DIFAT lists. A FAT sector id that is out of range, a chain loop, and every other structural fault stay refused. Each tolerated deviation adds the warning `container.cfb-tolerated`, which names the deviation.

### 2.2 The `PARASOLID` storage

The root storage has a child storage named `PARASOLID`. A document without this storage has no Parasolid body.

A stream of the `PARASOLID` storage has the name `<stem>.<ext>`. The stem and the extension compare without regard to case.

| Element | Meaning                                                              |
| ------- | -------------------------------------------------------------------- |
| `P_B`   | Extension of a Parasolid partition transmit stream.                  |
| `D_B`   | Extension of a Parasolid deltas transmit stream.                     |
| `X_B`   | Extension of a standalone Parasolid transmit stream.                 |

The stems `SKETCH` and `SVSKETCH` mark a sketch stream, whatever its extension. A sketch stream holds no body. Every other stem names a body group. A stem of the form `STREAM<n>`, with a non-empty decimal index `<n>`, is a numbered body group. Other stems, for example `ASSEMBLYMODEL`, are named body groups. An entry with another extension is not a transmit stream.

### 2.3 Body groups

The streams that share one stem form a body group. In a group, a partition stream is the base state and a deltas stream is an edit overlay that applies to that base state.

A standalone transmit stream is a base state with no overlay. A group with a partition stream and a standalone stream takes the partition stream as the base state.

The decoder reads every body group that has a base state. Numbered groups come before named groups. Numbered groups compare by index and named groups compare by stem. A group with a deltas stream and no base state yields no body.

The first group in the order above that has a base state is the design body. The decoder transfers its bodies. It does not transfer the bodies of a later group, and it adds the warning `container.body-group-untransferred` for each later group. The streams of every group stay retained as native records, so no data of a later group is lost. A later group is, for example, a flat pattern (`STREAM<n>FLAT<m>`), reference geometry (`REFERENCE`), or a design region (`DESIGNREGIONS`). An export of such a group would add bodies that the design does not have. The choice of the first group is a fallback and not a rule from the file (SE-01). The identities of the transferred group are the identities of the shared Parasolid decoder, `sldprt:brep:<kind>#<key>`.

---

## 3. Parasolid streams

### 3.1 Transmit header

A stream is a Parasolid transmit file. It has an optional text header and the binary transmit data.

**Text header.** The text header is the transmit file header of the public Siemens *Parasolid XT Format Reference*. Its lines begin with `**`: a line of the alphabet, a `**PARASOLID` line, and the parts `**PART1;`, `**PART2;`, and `**PART3;`. Part 2 holds the line `SCH=<schema token>`. Part 1 can hold the line `FORMAT=<value>`. The line `**END_OF_HEADER` followed by asterisks ends the header. The binary transmit data starts after the asterisks, and after a line break when one follows. A stream that does not start with `**` has no text header and is binary transmit data.

The decoder takes the schema token from the `SCH=` line. A `FORMAT=` value `text` states that the data is the text encoding of a transmit file. The decoder does not read that encoding.

**Binary header.** The binary transmit data starts with the standard Parasolid framing:

```text
PS 00 00
desc_len    u16 BE
description ASCII[desc_len]
padding
schema_len  u8
schema      ASCII[schema_len]      ; SCH_<modeller>_<schema>_<format>
```

The description of a partition stream names `partition`. The description of a deltas stream names `deltas`. The header layout, the schema token, and the record grammar that follows are the Parasolid stream grammar of [`sldprt.md`](sldprt.md) sections 3 and 4. The decoder takes the role of a stream from its name, not from its description.

### 3.2 Partition and deltas merge

The partition records are the base set. The deltas records fill subordinate records that the partition lacks and update point records. A deltas record never replaces a partition topology record or a partition carrier record with the same identity. The merge, the topology ownership chain, and the carrier families are those of [`sldprt.md`](sldprt.md) section 4.

A deltas stream without a base state yields no body.

### 3.3 Transfer

The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves of the merged record set form the neutral B-rep. The streams of the decoded group are also retained as unknown records.

The shared decoder reads each hosted loop ring in the order of its link cells, and it takes the vertex-use cell of a coedge as the vertex where the coedge starts. The decoder of Solid Edge reverses this reading (SE-20). Each loop ring runs the other way. An edge that has no stored parameter range swaps its start and end vertices and keeps the senses of its coedges. An edge that keeps a range keeps its ends and flips the senses of its coedges. Then the loops of a planar face run anticlockwise about the face normal that the face record states, and an arc edge runs anticlockwise about its circle from its start vertex to its end vertex.

A hosted curve or surface record has a sense character after its five references: `+` (`0x2b`) or `-` (`0x2d`). The shared decoder uses the byte to find the record and does not keep it. On a hosted stream the decoder of Solid Edge keeps it for curves: a circle or an ellipse whose record has `-` runs against its natural parameterisation, and an edge without a stored parameter range runs along the curve's direction from its start vertex to its end vertex. The decoder writes such a conic with its axis negated and its reference direction kept, so every point of the curve stays and the edge takes the arc that the record means (SE-20).

A face on a torus whose boundary is two full circles round the torus axis is one of two bands of the tube: the arc from one circle to the other, or the rest of the tube. The loop directions and the face sense select one: the face lies to the left of each loop seen from its normal, and the natural torus normal points out of the tube. The decoder of Solid Edge flips the sense of such a face when the selected band spans more than half a turn of the tube (SE-21). A fillet spans less than half a turn.

On a hosted stream the decoder derives no parameter-space curves from the 3D edges. The derivation runs before the reversals above and reads each edge on its own, so a face across the seam of a periodic surface gets values on both sides of the seam. A reader of the STEP output derives the curves from the surfaces.

A Parasolid solid body states its infinite outside as a region that has no shell. The decoder removes that region from a body that has a region with a shell. A body with only empty regions keeps them.

---

## 4. Classification

| Row                       | Condition                                                |
| ------------------------- | -------------------------------------------------------- |
| `solidedge:cfb-parasolid` | The CFB directory has a `PARASOLID` storage.             |
| `solidedge:unknown`       | The CFB directory has no `PARASOLID` storage.            |

Detection requires the CFB signature and a directory path whose first component is `PARASOLID`, `DocVersion2`, `PSMroots`, or `FamilyInfo`. The `solidedge:unknown` row covers a document without a `PARASOLID` storage. It reports the directory and transfers no geometry.

A document without a body is a typed refusal and not an error:

| Structure                                                                 | Loss code                              |
| ------------------------------------------------------------------------- | -------------------------------------- |
| `FamilyInfo` at the root, no `PARASOLID`, `DocVersion2`, or `PSMroots`    | `container.not-a-model-document`       |
| No `PARASOLID` storage, other markers                                     | `container.no-parasolid-storage`       |
| `PARASOLID` storage with no body stream, for example sketch streams only  | `container.no-body-stream`             |
| `PARASOLID` storage with a deltas stream and no base state                | `container.no-partition-stream`        |
| Directory that the reader cannot list                                     | `container.unreadable`                 |
| Directory with a tolerated deviation from MS-CFB                          | `container.cfb-tolerated` (warning)    |
| Secondary body group, retained and not decoded                            | `container.body-group-untransferred`   |

A Compound File whose directory cannot be listed is a typed result. The loss `container.unreadable` names the failing structural check. Detection reports low confidence for such a file, because the root members are not readable.

A family-of-parts master is a document with the root member `FamilyInfo` and without a `PARASOLID` storage. It takes the loss `container.not-a-model-document`.

A `PARASOLID` storage with only sketch streams, or with no stream, takes the loss `container.no-body-stream`.

Each sketch stream, each group without a base state, and each other entry of the storage adds a warning-level loss. Each stream with a readable schema token adds one `parasolid:` layer, tagged by the stream path. The decoder takes the token from the text header first and from the binary header second. No Solid Edge schema grammar is verified separately, so every such layer is the residual `parasolid:unknown` row. The decode runs with the residual layer and adds the loss `source.kernel-dialect-unverified`.
