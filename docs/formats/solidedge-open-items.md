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

### SE-01. Roles of the body groups

**Question.** What does each body group of a document hold, and is any group a flat pattern, a construction body, or a configuration?

**Known.** `solidedge.md` §2.3 "The decoder reads every body group that has a base state." gives the selection. The decoder transfers each group as its own body set and does not rank them. Observation: 124 documents have more than one body group, namely 68 of 208 `.psm` files, 52 `.par` files, and 4 `.asm` files. Sheet-metal parts can store a flat pattern apart from the folded body. The decoder does not detect that case. About 40 parts name their group `STREAM<n>` with `<n>` other than 0, and about 8 use the spelling `Stream<n>`.

**Note.** The decoder takes the first group in its order, the lowest `STREAM<n>` index, as the design body. It does not decode the later groups, and it keeps their streams as native records. The customer exports show the active body only. In round 3 the decoder kept the later groups as hidden bodies, and a STEP export still wrote them with an `INVISIBILITY` entity. The later groups of 124 documents have the stems `STREAM<n>FLAT<m>` (68), `REFERENCE` (31), `STREAM<n>S<m>` (22), and `DESIGNREGIONS` (3). They are named groups, and none of them is a plain numbered group. The rule is a fallback. No stream, description, or attribute that names the active group is known.

**Need.** We must know which group is the design body and which is a flat pattern. We must know this to export one correct solid and to label the other groups. A comparison of the face counts of each group with the customer export of the same document tells if the first group is the design body.

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

**Known.** `solidedge.md` §2.3 "The decoder reads every body group that has a base state." applies to them as to any named group. Observation: a few `.asm` files hold these streams. The rule has no verification on a real document.

**Need.** We must know if the body is an assembly-level body or a merged copy of the components. We must know this to avoid a duplicate model in a scene.

### SE-07. Directory names that the reader refuses

**Question.** Which Compound Files with the CFB signature fail the directory read after the change that accepts `:`, `!`, and `\`, and which check fails?

**Known.** `solidedge.md` §2.1 "The reader accepts" gives the rule. The reader refuses `/` only. `solidedge.md` §2.1 "The reader tolerates" gives the second rule: the reader accepts a sibling tree with wrong node colours and a header that counts more FAT sectors than the DIFAT lists. Observation: 31 files (27 `.par`, 2 `.psm`, 2 `.asm`) failed the directory read before the name rule. They failed again after it: 19 files on "sibling tree contains adjacent red nodes" and 12 on "DIFAT does not match its declared FAT count". Their directories hold storages such as `S:Master`, `S:FamilyOfParts`, and `S:JSite`. The second rule accepts both defects. `solidedge.md` §4 "A Compound File whose directory cannot be listed" gives the typed result for a file that still fails. The loss names the check, for example a chain loop or an out-of-range sector.

**Need.** We must know which structural defect each remaining file has. The loss text of a rerun gives it.

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

## 5. Decoder behavior on real documents

### SE-16. Face-count deficit against customer STEP exports

**Question.** Why does the decoder emit fewer faces than the customer STEP export for some documents?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. Observation: of 152 comparable pairs, faces are equal in 88 and lower in 59. Edges are close, with a median edge ratio of 1.016. The loss `sldprt/geometry.face-support-surface-untyped` occurs in 287 converted files, but that loss does not drop a face: the face is emitted with an unknown-geometry surface. The decoder drops a face when none of its loops is kept, and it keeps a loop only when every edge of its ring is emitted. It dropped an edge silently when an endpoint vertex did not resolve and the carrier was not a circle. A ring edge on a closed ellipse or a closed B-spline now takes a seam vertex, as a circle did. The loss `sldprt/topology.boundary-unresolved` counts the edges, loops, and faces that are still dropped. It also splits the dropped edges by cause: an endpoint with a null vertex reference, an endpoint whose vertex record is absent from the tables, and an endpoint whose point record is absent. Only Solid Edge documents report it. Observation: the loss occurs in 1,018 files, and in the 41 paired files that have it the face count never equals the customer count. The topology tables come from a scanner of record families that was written for SolidWorks streams. The Solid Edge streams state the schema `SCH_2800143_28002`, which no SolidWorks stream states. A record layout that differs by schema is a candidate cause. The decoder applies the deltas stream to the partition records before it resolves vertices, and a deltas stream is missing in 52 documents.

**Note.** The magic `c2 bc 92 8f 99 6e 00 00` of a `00 12` record is the null real of the tolerance field, with the value -3.14158e13. A tolerant vertex states its own tolerance there, so the scan found no vertex-use for it. The scan now reads such a vertex when its point exists, and it states the tolerance in the model. In round 4 this cut the dropped edges from 175,301 to 665. Edge-use and bridge records have the same field. The scan now reads a tolerant bridge when a loop names it and a tolerant edge-use when a coedge names it. The loss now splits the dropped loops by cause: a coedge that the tables do not hold, a coedge whose edge was not emitted, and an edge without a canonical coedge. The 322 documents that drop loops and no edges need this split. The edge scan did not take a boundary edge of a sheet whose single coedge is reversed, because the edge had no forward coedge, and its loops fell with it. A sole reversed coedge now defines such an edge, with its ends swapped. The loss counts the edges without a forward coedge and the edges with several. A vertex record that was absent in round 4 may carry extra reference cells after the point cell, which move the tolerance field by two bytes per cell. The scan now looks for the field up to three cells later. Both rules are unverified on real files.

**Round 7.** The diagnostics on 510 documents with drops showed four classes. First, 763 of the 1,450 absent vertex records are `00 12` records in a layout whose tolerance field holds neither the null real nor a plausible tolerance, with the `00 1d` record of the named point 28 bytes on. Second, 253 are vertex indices that name a world point directly. Third, 2,758 of the 2,765 edges without a canonical coedge have one reversed coedge, and the rule of round 6 did not fire. The likely cause is an explicit canonical reference in the edge-use that names that same reversed coedge. Fourth, 16,197 edges have a canonical coedge in no ring of a kept face, and 63,540 faces end without loops. A hosted decode now reads the first two shapes after the scan, keeps a group of face-uses with one owner and distinct payloads as separate faces, attaches a loop that its face's chain did not reach to the face that owns it, and lets an explicit canonical reference to the sole reversed coedge define the edge with the ends as the coedge has them. These causes are inferred from counts, not from record values. The summary diagnostic line states how often each rule fired and why the walk of the remaining faces without loops stopped.

**Diagnostics.** Round 6 on real files left the dropped edges and loops almost unchanged, so the causes are not known. Setting the environment variable `CADMPEG_SE_DIAGNOSTICS` to any value but `0` makes the decode add report notes that start with `{"kind":`, one JSON line per dropped edge. Each line holds type codes, indices, offsets between records, flags, and counts, and no name, string, value, or coordinate. The line `drop` states, for an endpoint that did not resolve, what the stream holds at the vertex index and at the point index. The line `no_canonical` states the coedges that name an edge with no forward coedge, with their sense and the types of their loop and owners. The line `off_ring` states an edge whose canonical coedge is in no ring. The line `summary` repeats the loss counts, and the line `file` states the schema token and the `DocVersion2` code.

**Need.** We must know which edges stay unresolved on real documents. The counts of the new loss give the answer, and they decide whether another carrier type needs a ring rule.

### SE-17. Check failures on converted output

**Question.** Why do 50 converted documents fail the model check?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. Observation: 27 failures are `geometric_consistency` and 23 are `parameter_domain`. They co-occur with the loss `sldprt/topology.pcurve-carrier-off-surface`, which states that a NURBS edge carrier has vertex ranges off its bound surface.

**Need.** We must know if the defect is in the source geometry or in the decoder. We must know this before we trust those documents.

**Note.** In round 4 the failures rose from 73 to 243 documents, 196 of them `parameter_domain`, after the edges of tolerant vertices entered the model. The detail of the findings is not known. The decoder now runs the model range checks on its own output. It withholds every edge range and every coedge pcurve range that they reject, and it adds the loss `geometry.range-withheld` with the counts. A range that has an equivalent form is moved first: a circle or ellipse range that starts outside one turn moves by whole turns, and a B-spline range that overshoots its domain by at most one millionth of the width moves to the domain end. The loss counts the repaired ranges apart from the withheld ones. The seam vertex of a ring edge on a circle now sits at the start of the range, not at the reference direction, when the range starts elsewhere. The next rerun shows which ranges fail. The tolerant vertex now states its tolerance, which the check of edge ends against vertices uses.

### SE-18. Work budget of the surface solvers

**Question.** How much solver work do the global NURBS inversion and the chord certification need on real surfaces?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. Observation: 181 documents ended with a resource refusal: 110 in the geometry evaluation work slice and 71 in the certification of a NURBS chord. After the first change, 153 remained: 141 in the work slice and 12 in session work units. The failures take 0.24 s and do not follow the file size. A synthetic measurement shows the cost driver. A query for a point that is not on the surface has no answer, and the search then spends its whole slice. One 4 by 4 surface with a single span spent 3.6 million units on such a query. The span count therefore does not bound the cost of a failing query. The slice of a solver call is now advisory: when it runs out, the solver call returns no value, the decode goes on, and the loss `sldprt/geometry.solver-work-withheld` records the call. The session was fused before. A call never asks for more than the session has left. The slice grows with the spans of the surface, from 1,000,000 up to 8,000,000 units. After four calls that ran out of work, or after 24,000,000 units in all, the decode withholds the rest. The solver calls stop when the session has one third of its work allowance left, so that the records after them still find work. The first version of this rule gave the solvers one quarter of the allowance, and the withheld solves doubled in round 4, partly because the edges of tolerant vertices entered the model. The factors of 1,024 units per span, of 24,000,000 units, and of one third are assumptions. The 25 documents that still ended with a session work refusal in round 3 did so in later steps such as the collection of provenance, which suggests that the solvers had spent the session.

**Need.** We must know if the slice and the two limits suit real surfaces. A rerun shows how many documents report the loss and what that costs in time. We must also know why so many queries have no answer, because the same defect can cause the check failures of SE-17.

### SE-19. Decode timeout

**Question.** Which input makes a decode or a check exceed 60 seconds?

**Known.** `solidedge.md` §3.3 "The bodies, regions, shells, faces, loops, coedges, edges, vertices, points, surfaces, and curves" gives the transfer. Observation: one `.par` file of 1 MB to 16 MB with the schema `SCH_2800143_28002` timed out. A synthetic body of 1.3 MB with 1,500 triangles decodes in about 0.1 s in a release build, so the record scan is not quadratic in the record count. A solver call that finds no answer spends its whole slice (SE-18). The coedge selection of the topology scan looked up the loops of each coedge candidate with a scan over all loops. A stream of several megabytes holds many coedge candidates that share one 16-bit attribute, so that scan was quadratic in the number of loops. The scan now uses an index by attribute. The decode now stops all solver calls after 24,000,000 local units in total, which bounds their time.

**Need.** We must know the input structure that causes the timeout. Without the file, the change is a bound and not a fix. A rerun of the file shows if the bound is enough.
