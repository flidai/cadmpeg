# Siemens Solid Edge `.par`, `.psm`, and `.asm` (CFB + Parasolid): Format Specification

> **License:** This document is released under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/). Attribute to the cadmpeg project.

---

## 1. Format overview

A Solid Edge part (`.par`), sheet-metal part (`.psm`), or assembly (`.asm`) is a Compound File Binary (CFB) document as defined by MS-CFB. The B-rep of the document is a Parasolid partition stream and a Parasolid deltas stream. Both streams are members of one storage named `PARASOLID`.

**Byte order and units (global):**

- CFB header, FAT, directory, and sector fields are **little-endian**.
- Parasolid neutral-binary payload fields are **big-endian**.
- Parasolid geometric doubles are in **metres**. Model geometry in the neutral model is millimetres (×1000).

The file extension is not a format discriminant. A CFB document is a Solid Edge document with a Parasolid body exactly when its directory has a `PARASOLID` storage (section 2.2).

---

## 2. Container

### 2.1 Compound File

The container follows MS-CFB. Stream and storage names are compared without regard to case, by the MS-CFB name ordering: names compare by UTF-16 length first and then by uppercased code units. A stream shorter than the mini-stream cutoff is stored in the mini stream and is read through the mini-FAT. Any other stream is stored in regular sectors.

### 2.2 The `PARASOLID` storage

The root storage has a child storage named `PARASOLID`. A document without this storage has no Parasolid body.

Streams of the `PARASOLID` storage have the name `STREAM<n>.<ext>`:

| Element | Meaning                                                            |
| ------- | ------------------------------------------------------------------ |
| `<n>`   | Non-empty decimal index.                                           |
| `P_B`   | Extension of a Parasolid partition transmit stream (binary).        |
| `D_B`   | Extension of a Parasolid deltas transmit stream (binary).           |

The pair `STREAM0.P_B` and `STREAM0.D_B` is the body of the document. The partition stream is the base state. The deltas stream is an edit overlay that applies to that base state.

---

## 3. Parasolid streams

### 3.1 Stream header

Each stream starts with the standard Parasolid transmit header:

```text
PS 00 00
desc_len    u16 BE
description ASCII[desc_len]
padding
schema_len  u8
schema      ASCII[schema_len]      ; SCH_<modeller>_<schema>_<format>
```

The description of a partition stream names `partition`. The description of a deltas stream names `deltas`. The header layout, the schema token, and the record grammar that follows are the Parasolid stream grammar of [`sldprt.md`](sldprt.md) sections 3 and 4.

### 3.2 Partition and deltas merge

The partition records are the base set. The deltas records fill subordinate records that the partition lacks and update point records. A deltas record never replaces a partition topology record or a partition carrier record with the same identity. The merge, the topology ownership chain, and the carrier families are those of [`sldprt.md`](sldprt.md) section 4.

A deltas stream without a partition stream has no base state and yields no body.

### 3.3 Transfer

The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves of the merged record set form the neutral B-rep. The two streams are also retained as unknown records.

---

## 4. Classification

| Row                       | Condition                                                |
| ------------------------- | -------------------------------------------------------- |
| `solidedge:cfb-parasolid` | The CFB directory has a `PARASOLID` storage.             |
| `solidedge:unknown`       | The CFB directory has no `PARASOLID` storage.            |

Detection requires the CFB signature and a directory path whose first component is `PARASOLID`. The `solidedge:unknown` row is reachable only when the caller forces the Solid Edge decoder on a CFB document. It reports the directory and transfers no geometry.

Each stream with a readable transmit header adds one `parasolid:` layer, tagged by the stream path. No Solid Edge schema grammar is verified separately, so every such layer is the residual `parasolid:unknown` row.
