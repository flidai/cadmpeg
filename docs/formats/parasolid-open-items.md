# Parasolid Transmit File Open Items

This document lists the parts of the Parasolid transmit file format (`.x_b` and `.x_t`) that we do not know. The specification `parasolid.md` gives the parts that we know.

Each item has these parts:

- **Question** — what we must find.
- **Known** — what the specification gives now, and observations that the specification does not state.
- **Need** — why we must find the answer.
- **Conflict** — a disagreement between two documents, or between a document and the decoder. An item with this part needs a decision.
- **Note** — a defect in the item or in the specification.

When an item is resolved, delete it in the same change that writes the answer into the specification. Do not keep a Resolved part.

Each item has an identifier. Use the identifier in commit messages and in code comments.

This document uses ASD-STE100 Simplified Technical English. Record names, field names, and token values are technical names. They keep their source spelling.

## 1. Encodings

### PS-01. Text transmit data

**Question.** Which field order, token grammar, and per-node layout does the text encoding of the transmit data use, and can the binary record reader of the shared decoder read it?

**Known.** `parasolid.md` §4 gives the first record line. The decoder reads the header of a file that declares `FORMAT=text`, classifies its schema token, and refuses the file with the loss `parasolid.text-transmit-unsupported`. Observation: 51 `.x_t` files carry `FORMAT=text`, and each holds one record line `T50` or `T51` after the header. The shared decoder does not read a schema-driven record layout. It recognizes the binary node types by their tags and magic values, as `sldprt.md` section 4 states. The text data has no such tags, so the shared decoder cannot read it.

**Need.** We must know the node types, the order of the fields of each type, and the token encodings of integers, reals, logicals, and characters. We must know this to write a text reader. Without it, no `.x_t` file converts.

### PS-02. Binary data of a standalone file

**Question.** Does the binary part that follows `**END_OF_HEADER` use the framing of `parasolid.md` §3 for every schema?

**Known.** `parasolid.md` §3 gives the layout that the decoder reads. The layout comes from the SolidWorks streams, which hold the same binary data without the text header. No real standalone binary transmit file is checked in. The decoder reports a file whose binary part it cannot read as `parasolid.header-unreadable`, and it keeps the schema token of the text header.

**Need.** We must know if the framing is the same behind the text header. If it is not, no `.x_b` file converts.

### PS-03. Binary transmit data without a text header

**Question.** Do standalone files exist that hold binary transmit data and no text header?

**Known.** The decoder detects a file by its text header. Binary data that starts with `PS 00 00` is not detected as a transmit file. The Solid Edge codec reads such a stream inside a Compound File.

**Need.** We must know if the case occurs. If it does, detection must also accept the binary signature.
