# Siemens Solid Edge Open Items

This document lists the parts of the Solid Edge `.par`, `.psm`, and `.asm` format that we do not know. The specification `solidedge.md` gives the parts that we know.

Each item has these parts:

- **Question** — what we must find.
- **Known** — what the specification gives now, and observations that the specification does not state.
- **Need** — why we must find the answer.
- **Conflict** — a disagreement between two documents, or between a document and the decoder. An item with this part needs a decision.
- **Note** — a defect in the item or in the specification.

When an item is resolved, delete it in the same change that writes the answer into the specification. Do not keep a Resolved part.

Each item has an identifier. Use the identifier in commit messages and in code comments.

This document uses ASD-STE100 Simplified Technical English. Record names, field names, and token values are technical names. They keep their source spelling.

## 1. Container

### SE-01. Additional Parasolid streams

**Question.** What do the streams `STREAM1` and higher in the `PARASOLID` storage hold, and does the storage hold entries with other names?

**Known.** `solidedge.md` §2.2 "Streams of the `PARASOLID` storage have the name" gives the name pattern. The decoder reads only the `STREAM0` pair and reports every other entry of the storage as not transferred. Observation: the census that counted `PARASOLID/STREAM0.P_B` did not count other streams.

**Need.** We must know whether a document with more than one stream pair holds more than one body, a body per configuration, or a construction body. We must know this to transfer the right body.

### SE-02. Role of the deltas stream

**Question.** What does `STREAM0.D_B` hold, and is the partition stream alone a complete body state?

**Known.** `solidedge.md` §3.2 "The partition records are the base set." gives the merge that the decoder applies. Observation: `STREAM0.D_B` is present exactly when `STREAM0.P_B` is present. It is present in 97% of `.par` files, 99% of `.psm` files, 11% of `.asm` files, and 0% of `.dft` drawings.

**Need.** We must know if the deltas stream changes the geometry of the final body. We must know this to decide whether a document without a deltas stream is complete.

### SE-03. Description strings of the pair

**Question.** Does the description of every `P_B` stream name `partition` and the description of every `D_B` stream name `deltas`?

**Known.** `solidedge.md` §3.1 "The description of a partition stream names" states the rule. The decoder takes the partition-or-deltas role from the description, not from the stream name.

**Need.** We must know if the stream name or the description decides the role when they disagree.

## 2. Versions

### SE-04. Writing-application versions and schema tokens

**Question.** Which Parasolid schema tokens and transmit formats occur in documents of each Solid Edge release, and does the record grammar differ between them?

**Known.** `solidedge.md` §3.1 "Each stream starts with the standard Parasolid transmit header" gives the header. Observation: documents range from Solid Edge V18 to Solid Edge 2023. The root stream `DocVersion2` has 4 bytes, and its second u16 is a version code: 18 is V18, 102 to 110 are ST2 to ST10, and 219, 221, and 223 are 2019, 2021, and 2023. The decoder reads no version code. Every schema layer is the residual `parasolid:unknown` row.

**Need.** We must know which schema tokens occur to add verified rows. We must know whether older schemas need a different record grammar.

### SE-05. Detection of documents without a Parasolid storage

**Question.** Which structure identifies a Solid Edge document that has no `PARASOLID` storage?

**Known.** `solidedge.md` §4 "Detection requires the CFB signature" gives the rule. Observation: most `.asm` files carry no own B-rep, and `.dft` drawings never carry a `PARASOLID` storage. The root stream `DocVersion2` is present in the observed documents.

**Need.** We must know a discriminant for these documents. Without it, a decode of such a document needs a forced Solid Edge decoder and detection reports no format.

## 3. Model

### SE-06. Length unit of the document

**Question.** Where does a Solid Edge document state its length unit, and does the unit change the Parasolid values?

**Known.** `solidedge.md` §1 "Parasolid geometric doubles are in **metres**" gives the stream unit. The decoder scales every length by 1000 and reads no document unit.

**Need.** We must know if any document stores Parasolid values in a different unit. We must know this to state lengths correctly.

### SE-07. Body selection

**Question.** Which bodies of a partition are the model solid, and which are construction, surface, or hidden bodies?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. The decoder transfers every body of the merged record set.

**Need.** We must know how to separate the model body from other bodies. We must know this to export one correct solid.

### SE-08. Face colors and attributes

**Question.** How does Solid Edge store face colors, materials, and other attributes of the body?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. The decoder transfers no color. It reports each face-color record that the shared Parasolid decoder finds.

**Need.** We must know where the attributes are stored. We must know this to transfer appearance.

### SE-09. Assembly bodies

**Question.** What B-rep does a `.asm` file with a `PARASOLID` storage hold, and how does it relate to the occurrences of the assembly?

**Known.** `solidedge.md` §1 "A Solid Edge part (`.par`), sheet-metal part (`.psm`), or assembly (`.asm`) is a Compound File Binary" gives the container. Observation: `STREAM0.P_B` is present in 11% of `.asm` files. The decoder reads no occurrence, placement, or assembly structure.

**Need.** We must know if the body is an assembly-level feature body or a merged copy of the components. We must know this to avoid a duplicate model in a scene.
