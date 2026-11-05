# Parasolid transmit files (`.x_b`, `.x_t`): Format Specification

This document states the structure of a standalone Parasolid transmit file. The file holds one Parasolid partition. This document uses ASD-STE100 Simplified Technical English. Record names, field names, and token values are technical names. They keep their source spelling.

## 1. Format overview

A transmit file has a text header and the transmit data. The text header is the transmit file header of the public Siemens *Parasolid XT Format Reference*. The transmit data is binary or text, as the header declares. The extension `.x_b` names a file with binary data. The extension `.x_t` names a file with text data.

- Parasolid binary transmit fields are **big-endian**.
- Parasolid lengths are metres.
- Content selects the format. A file is a transmit file if it starts with the first two header lines.

## 2. Text header

The text header is a block of lines. Each line begins with `**` or is a field line of a part.

```text
**ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz***...
**PARASOLID !"#$%&'()*+,-./:;<=>?@[\]^_`{|}~0123456789***...
**PART1;
<field>=<value>;           ; zero or more lines
**PART2;
SCH=<schema token>;
USFLD_SIZE=<n>;
**PART3;
**END_OF_HEADER***...
```

The first line is the alphabet line. The second line starts with `**PARASOLID`. Both lines end with asterisks. A line break is `LF` or `CR LF`.

Part 1 holds field lines. A field line is `<name>=<value>` with an optional trailing `;`. The field `FORMAT` has the value `binary` or `text` and declares the encoding of the transmit data. The fields `APPL`, `SITE`, `USER`, `OS`, `MC`, `FRU`, `KEY`, `FILE`, and `DATE` name the writing application and host.

Part 2 holds the line `SCH=<schema token>`. A schema token has the form `SCH_<modeller>_<schema>`, where the characters after `SCH_` are ASCII letters, digits, and underscores.

The line `**END_OF_HEADER` followed by asterisks ends the header. The transmit data starts after the asterisks, and after a line break when one follows.

## 3. Binary transmit data

Binary transmit data starts with the standard Parasolid framing:

```text
PS 00 00
desc_len    u16 BE
description ASCII[desc_len]
padding
schema_len  u8
schema      ASCII[schema_len]      ; SCH_<modeller>_<schema>_<format>
```

The framing, the schema token, and the record grammar that follows are the Parasolid stream grammar of [`sldprt.md`](sldprt.md) sections 3 and 4. The binary part is one stream, and its records are the base set.

## 4. Text transmit data

Text transmit data starts with a record line of the form `T<n> : TRANSMIT FILE created by modeller version <version> <schema token> <n>`. The fields of the records that follow are separated by spaces.

## 5. Transfer

The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves of the binary record set form the neutral B-rep. The file is also retained as an unknown record.

---

## 6. Classification

The file has one dialect layer: the `parasolid:` row that the schema token names, or `parasolid:unknown`. The schema token is the `SCH=` value, or the binary framing schema when the header has none. The layer declares `schema`, `carrier` (the value `transmit`), and `encoding` (`binary` or `text`).
