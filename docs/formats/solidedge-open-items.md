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

### SE-01. Additional body groups

**Question.** Does a document hold more than one body group, and what does each group hold?

**Known.** `solidedge.md` §2.3 "The decoder reads one body group" gives the selection. The decoder reads the first group that has a base state. It reports every other group as not transferred. Observation: about 40 parts name their pair `STREAM<n>` with `<n>` other than 0, and about 8 use the spelling `Stream<n>`. The number of documents with two body groups is not known.

**Need.** We must know whether a second group is a second body, a configuration, or a construction body. We must know this to transfer the right bodies. The entity identities that the shared decoder makes come from record numbers of one stream. Two groups in one model need a rescoping of those identities first.

### SE-02. Role of the deltas stream

**Question.** What does a deltas stream hold, and is the partition stream alone a complete body state?

**Known.** `solidedge.md` §3.2 "The partition records are the base set." gives the merge that the decoder applies. Observation: a deltas stream is present exactly when a partition stream is present. It is present in 97% of `.par` files, 99% of `.psm` files, 11% of `.asm` files, and 0% of `.dft` drawings.

**Need.** We must know if the deltas stream changes the geometry of the final body. We must know this to decide whether a document without a deltas stream is complete.

### SE-03. Description strings of the streams

**Question.** Does the description of every `P_B` stream name `partition` and the description of every `D_B` stream name `deltas`?

**Known.** `solidedge.md` §3.1 "The description of a partition stream names" gives the descriptions. The decoder takes the role of a stream from its name, not from its description. The shared decoder orders the streams of a group by the word `deltas` in the description.

**Need.** We must know if the stream name or the description decides the role when they disagree.

### SE-04. Standalone transmit streams

**Question.** Is a stream with the extension `X_B` a complete Parasolid transmit file, and does it use the partition record grammar?

**Known.** `solidedge.md` §2.3 "A standalone transmit stream is a base state" gives the rule that the decoder applies. Observation: about 26 parts hold an `X_B` stream and no other body stream. The rule has no verification on a real document.

**Need.** We must know if the shared decoder reads the records of an `X_B` stream. Without this, those parts convert to nothing.

### SE-05. Sketch streams

**Question.** What do the streams `SKETCH.P_B` and `SVSKETCH.P_B` hold?

**Known.** `solidedge.md` §2.2 "The stems `SKETCH` and `SVSKETCH` mark a sketch stream" gives the classification. The decoder reports each such stream as not transferred and reads none. Observation: `SVSKETCH.P_B` is the only stream of the storage in about 70% of `.asm` files and `SKETCH.P_B` in about 5%. Twelve parts hold only `SKETCH.P_B`.

**Need.** We must know if a sketch stream holds curves that the neutral model can take. We must know this to transfer sketches.

### SE-06. Assembly model streams

**Question.** What do `ASSEMBLYMODEL.P_B` and `ASSEMBLYMODEL.D_B` hold, and do they follow the grammar of a part body?

**Known.** `solidedge.md` §2.3 "The decoder reads one body group" applies to them as to any named group. Observation: a few `.asm` files hold these streams. The rule has no verification on a real document.

**Need.** We must know if the body is an assembly-level body or a merged copy of the components. We must know this to avoid a duplicate model in a scene.

### SE-07. Directory names that the reader refuses

**Question.** Which Compound Files with the CFB signature fail the directory read after the change that accepts `:`, `!`, and `\`?

**Known.** `solidedge.md` §2.1 "The reader accepts" gives the rule. The reader refuses `/` only. Observation: 31 files (27 `.par`, 2 `.psm`, 2 `.asm`) failed the directory read before the change. Their directories hold storages such as `S:Master`, `S:FamilyOfParts`, and `S:JSite`. The change is not tested on those files.

**Need.** We must know if the change reads all 31 files. A name with `/`, or another defect, would still fail.

## 2. Versions

### SE-08. Writing-application versions and schema tokens

**Question.** Which Parasolid schema tokens and transmit formats occur in documents of each Solid Edge release, and does the record grammar differ between them?

**Known.** `solidedge.md` §3.1 "The decoder takes the schema token from the `SCH=` line." gives the source of the token. Observation: documents range from Solid Edge V18 to Solid Edge 2023. The root stream `DocVersion2` has 4 bytes, and its second u16 is a version code: 18 is V18, 102 to 110 are ST2 to ST10, and 219, 221, and 223 are 2019, 2021, and 2023. The decoder reports that value as a note and does not branch on it. The observed tokens are 38 distinct values from `SCH_1400184_14000` to `SCH_3501256_35102`. The most common are `SCH_1700272_16100`, `SCH_2201216_20000`, `SCH_3001278_30100`, `SCH_3500239_35002`, and `SCH_2600169_25001`. Every schema layer is the residual `parasolid:unknown` row, and the decode runs with the loss `source.kernel-dialect-unverified`.

**Need.** We must know which schema tokens decode correctly, to add verified rows. We must know whether older schemas need a different record grammar.

### SE-09. Binary data behind the text header

**Question.** Does the binary part that follows `**END_OF_HEADER` use the framing of `solidedge.md` §3.1 for every schema?

**Known.** `solidedge.md` §3.1 "**Binary header.**" gives the layout that the decoder reads. The layout comes from the SolidWorks streams, which hold the same binary data without the text header. No real Solid Edge stream is checked in. The decoder reports a stream whose binary part it cannot read as `parasolid.header-unreadable`, and it keeps the schema token of the text header.

**Need.** We must know if the framing is the same behind the text header. If it is not, no document converts.

### SE-10. Text-encoded transmit data

**Question.** Does any document hold transmit data with `FORMAT=text`?

**Known.** `solidedge.md` §3.1 "The decoder takes the schema token from the `SCH=` line." gives the rule: a `FORMAT=` value `text` states that the data is the text encoding of a transmit file. The decoder does not read that encoding and reports `parasolid.header-unreadable`. No document with this value is known.

**Need.** We must know if the case occurs. If it does, a text reader is a separate task.

## 3. Documents without a body

### SE-11. Family-of-parts masters

**Question.** What does a family-of-parts master hold, and does a member document carry the geometry of the family?

**Known.** `solidedge.md` §4 "A family-of-parts master is" gives the typed refusal. Observation: 259 `.asm` files have the streams `FamilyInfo` and `FamilyPropertyFlags` and a storage `S:Master`, with no `PARASOLID` storage, `PSMroots`, or `DocVersion2`.

**Need.** We must know if the geometry of a family is in the master or in the member documents. We must know this to transfer family members.

### SE-12. Assemblies without own geometry

**Question.** Which structure of an `.asm` file lists its occurrences, and where is the placement of each occurrence?

**Known.** `solidedge.md` §4 "A `PARASOLID` storage with only sketch streams" gives the typed refusal for a storage without a body stream. Observation: 3,761 `.asm` files have a `PARASOLID` storage with only sketch streams or with no stream. The decoder reads no occurrence, placement, or assembly structure.

**Need.** We must know the assembly structure to transfer an assembly as occurrences of its part documents.

## 4. Model

### SE-13. Length unit of the document

**Question.** Where does a Solid Edge document state its length unit, and does the unit change the Parasolid values?

**Known.** `solidedge.md` §1 "Parasolid geometric doubles are in **metres**" gives the stream unit. The decoder scales every length by 1000 and reads no document unit.

**Need.** We must know if any document stores Parasolid values in a different unit. We must know this to state lengths correctly.

### SE-14. Body selection

**Question.** Which bodies of a partition are the model solid, and which are construction, surface, or hidden bodies?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. The decoder transfers every body of the merged record set.

**Need.** We must know how to separate the model body from other bodies. We must know this to export one correct solid.

### SE-15. Face colors and attributes

**Question.** How does Solid Edge store face colors, materials, and other attributes of the body?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. The decoder transfers no color. It reports each face-color record that the shared Parasolid decoder finds.

**Need.** We must know where the attributes are stored. We must know this to transfer appearance.
