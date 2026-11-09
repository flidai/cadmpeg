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

## State of the codec

Status after round 14. The numbers come from the rescores on the real documents of the coordinator. The author of the decoder has no real document.

**Proven on real documents.**

- The container reads through a tolerant CFB reader. The `PARASOLID` streams go to the shared sldprt Parasolid decoder, and `cadmpeg convert x.par --to step` works. Of 9,259 documents, 4,977 convert. Most of the other documents have no body stream or are assemblies without own geometry (SE-11, SE-12).
- The numbering of references is the identity. A reference carries no mask and no offset.
- A pointer above `0x7ffe` takes two big-endian cells: `(first & 0x7fff) | (second << 15)`. A record is two bytes longer for each wide pointer. The scan with fixed offsets misses these records, and the second pass reads them.
- A header of the expected type sits at the referenced index for nearly all references that the tables leave dangling. The second pass admits these records until no new index appears. It admits a vertex together with its point, and it admits a coedge only if the twin it names agrees.
- The faces of converted documents rose from 422,000 to 481,401 over rounds 9 to 10. Round 11 lost 2,806 faces in 8 documents. Round 12 binds such faces to the shell of the faces that share an edge with them.

- Round 13 measured the stored-versus-computed check on the 5,029 `.par` and 208 `.psm` documents of the real share. A hosted scan rejects a point record whose coordinate is a nonzero double smaller than 1e-30 in magnitude: such a double is a record of the deltas stream that was read from the wrong place (SE-16). The change raised the complete `.par` models from 1,101 to 1,115 and lowered `face:edge_range_off_vertices` from 1,557 to 1,476 files. The complete `.psm` models stay at 16 of 208. The count of compared models stays at 335, so the new complete models have no stored volume to compare, and the agreement counts are unchanged: of 335 compared models 192 agree within 0.1 percent and 117 differ by more than 25 percent. The timeouts of the converter went from 1 to 2 files. The cause of the second is not known.

- Round 14 measured two changes on the same documents. The numbers in this bullet need the checker patch of the coordinator (the torus-band rule below), which is not in this repository. A hosted decode now reads a compact analytic record whose pointers are wide (SE-16). On the real share, one in four of the partial `.par` documents listed 23,780 curve references and 7,305 surface references that the decode could not type; after the change 13,618 and 4,939 remain. The complete models and the agreement with the stored values did not change. `loss:edge_curve_untyped` stayed at 889 files and `loss:face_surface_untyped` went from 707 to 700. With the torus-band rule in the checker, the complete `.par` models rise from 1,115 to 1,698 (`.psm` from 16 to 24), the compared models from 335 to 526, the models within 0.1 percent of the stored volume from 192 to 326, and the models more than 25 percent off from 117 to 160. Of the 43 added models that are more than 25 percent off, 36 are among the 42 compared models with an undecided body for which no reading of the body comes within 25 percent of the stored volume (round 15, joined per file on the server); the other 7 are far off although a reading within 25 percent exists. The thresholds of the rule were chosen on the same files that these numbers come from; the holdout of round 15 is in SE-16.

**Open.**

- Face deficit against the customer STEP exports (SE-16). The `ref` lines show what remains: 725 rings that stop at a coedge whose loop field names no loop, 3,835 rings that stop at a coedge with no readable record, 8,938 faces whose loops were all dropped later, and 12,899 faces that no body names.
- Check failures on converted output (SE-17), solver work (SE-18), and one timeout of about 60 s (SE-19).
- The body groups, the role of the deltas stream, and the family-of-parts masters (SE-01, SE-02, SE-11).

**Next leads.**

1. The loop field of a coedge. In 725 foreign-loop rings, the field names a loop on 19 lines, and it never names the loop of the ring. It names a coedge on 126 lines and an edge-use on 96 lines. In 686 of the 1,280 coedges at the stop the sense marker is at its normal place, so the length of the record is right. The other candidates are a different field order in some schema versions, or an index that exists in both the partition and the deltas stream with different content (572 of the 1,280 have a header in the deltas stream). The `ref` lines with the bytes of the record would decide this. They need a real document.
2. The owner field of a bridge. All 3,570 references are untried. Only 229 have a header of type 14. The owner most likely names the typed face node that `typed.rs` already reads, so the second pass must not read it as a bridge.
3. Rings that stop at an absent coedge although a header is present (3,835). The pointer rule explains most. The rest likely use the layout with a tripled reference.
4. Vertex and edge drops (`absent_vertex` 6,071). A vertex that has no readable point is the main part.
5. Control documents (without the loss) still admit records by reference. The admitted records are real records that a table names, but the controls show the second pass changes documents that the scan read cleanly. Round 12 does not change this.

6. Round 13 order of work, by files that become complete when a reason is gone (3,911 partial `.par` files). `shell:region_side_undecided` is the only reason of 746 files. Then come `face:loop_not_closed_in_parameters` (328 alone, 1,074 with the first), `face:edge_range_off_vertices` (314 alone, 1,617 cumulative), and `face:wrapping_loops_unsupported` (154 alone, 2,009 cumulative). The reasons come in groups: 229 files need both `edge_range_off_vertices` and `loop_not_closed_in_parameters`, and 113 need those two and `wrapping_loops_unsupported`. Fixing a reason alone frees few files.
7. `shell:region_side_undecided` is a limit of the checker, not a defect of the codec. The integrator ignores the face sense and the ring direction on purpose. A sphere cap or a torus band then has two readings. A tie-break by the coedge sense for rings and by the face sense for caps made 726 more `.par` models complete on the real share, but 100 more models differ from the stored volume by more than 25 percent. That change belongs to the checker. It needs a decision of the owner of the checker, and it is not in the codec.
8. Untyped carriers (`loss:edge_curve_untyped` 889 files, `loss:face_surface_untyped` 706 files). The `summary3` lines of the diagnostics (below) count the type code of the record at the index of each untyped reference, on one tenth of the partial files (7,494 curve references and 2,641 surface references). For curves, 3,425 of the references name a record of type code 30 or 31, 1,061 name type code 38, and 319 name type code 45. For surfaces, the largest groups are the type codes 45 (456), 43 (402), 56 (386), 50 (344), 51 (190), and 63 (141). 277 curve references and 29 surface references have no header in any stream. 1,942 curve references and 425 surface references sit at a header that is not of a carrier type. The type codes 30 and 31 hold 3,425 of the 7,494 curve references, so they come first. The data do not yet state why the decoder leaves a record of such a type untyped. The next step is a `ref`-style line for these references with the distance to the nearest carrier key, as for vertices in round 9.
9. Not tried: polyline B-curves, wrapping loops, and the periodic B-spline surfaces (`face:surface_periodic_bspline`, 475 files, 28 alone).

10. Round 14 order of work, after the torus-band rule: `face:loop_not_closed_in_parameters` is the only reason of 328 files, `face:edge_range_off_vertices` of 314, `face:wrapping_loops_unsupported` of 154, and `shell:region_side_undecided` of 149 (down from 746). The first two together free 871 files, and with the wrapping loops 1,263.
11. Untyped carriers after the wide-pointer change. In one in four of the partial files, 13,618 curve and 4,939 surface references are still untyped. The record type at the index says which family reads them next. For curves: 4,256 name the intersection record (type code 38, `00 26`), 960 name a control array (45, `00 2d`), 5,594 sit at a header that is not of a carrier type, and 989 have no header. For surfaces: 1,301 name the blend record (56, `00 38`), 680 name a control array, 498 type code 43, and 1,554 sit at another header. The scanners of the intersection, blend, offset, and spline families read their references at fixed offsets, so they miss a record with wide pointers as the analytic scan did. The deltas framing has the same defect: 641 line and 544 circle records in the deltas stream have no marker within 30 bytes of the strict position that leads to valid values, so the deltas layout of a wide pointer is not the `[hi][lo][01]` triple. The next step is the wide-pointer reading in these scanners, with the line `summary4` as the measure.
12. `shell:region_side_undecided` after the rule: 152 files remain. They are caps on spheres, torus bands whose two arcs are within 25 degrees of half a turn on both sides, and bodies with more than ten undecided faces. Among the 44 models with a torus band whose first arc lies in the zone 170 to 220 degrees, the stored volume agrees with the other arc for 36. A single limit of 170 degrees, with the other arc taken above it, would decide them. Round 15 measured it on the whole share: see SE-16. It is not better than the 170/220 rule in complete models, and it adds exact models (326 to 357 of 524 and 567 compared) at 7 more far-off models.
13. Round 17 decides the 126 far-off models with a stored density of zero (SE-16, Round 17): 381 documents carry one identical stored mass set, and its value is foreign to their geometry. The customer STEP exports cover none of the 126; the reproducible count of STEP pairs is 181, not 152.
14. Round 16: the face senses of curved faces. The senses that the records state agree with the side found from the edges for 98.5 percent of the planar faces and for 69 to 80 percent of the cylinder, cone, and torus faces of the models that match their stored volume. The ring coedges give the opposite relation to the one the open coedges give. Both are unexplained.
15. Round 16: the closed B-spline edges that close only to the fit tolerance (cylinder and cone loops) and the 60 shells that then do not close.

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

Round 7. The first round of diagnostics on 510 documents with drops showed that references keep naming records that no table holds: 763 vertex records in a layout without a usable tolerance field, 2,758 edges with one reversed coedge, 16,197 edges whose canonical coedge is in no ring of a kept face, and 63,540 faces without loops. The rules that the decoder added in response did not help. A loop that a face's chain did not reach, once attached to the face that owns it, turned clean documents into lossy ones. The group of face-uses with one owner never occurred. The vertex layout never matched. A vertex index that is also a world point key is likely a coincidence of two independent numberings. The decoder removed those rules. It kept one: an explicit canonical reference to the sole reversed coedge of an edge defines the edge with the ends as the coedge has them. Of 3,286 `no_canonical` lines, 3,263 carry an explicit reference that names no coedge. The common thread is that the numbering of references does not match the keys of the tables. The next rerun answers this with the `ref` and `keys` lines below.

Round 9. The `ref` lines of round 8 showed that the numbering is the identity: references carry no mask and no offset. A header of the expected type sits at the referenced index for 100 percent of vertex references and 99.8 percent of canonical-coedge and first-loop references, and no table held the record. A hosted decode now runs a second pass over the indices that records in the tables name and the tables do not hold. Round 9 on the real files converted two more documents, fixed 73 of 510 loss reports, and added 3,580 faces in 198 documents. The remaining records show the sense marker 2, 4, 6, or 8 bytes after the strict position, and 470 of the 1,383 remaining coedge references are above `0x7fff`. That fits the pointer rule of the XT nodes (`sldprt.md` section 4): a pointer above `0x7ffe` takes two cells, so a record is longer by two bytes per wide pointer, and the fixed offsets of the scan put the marker and the tolerance field in the wrong place. The decoder reads the typed nodes this way already. Round 10 reads a record in the second pass with a cursor that follows wide pointers, keeps a wide value only when it fits sixteen bits because the tables are keyed by sixteen bits, and runs until no admitted record names a new index, with the work limit of the session as the only bound. Where an index has several different readings, the reading that most references of the tables agree with wins, then the only reading from the partition stream. A face-use whose owner is already held is not admitted again. A bridge that was admitted by reference and has no typed face node stays out of the typed body hierarchy, because one such bridge made the hierarchy incomplete and merged the bodies of seven documents into one. A vertex is admitted without a tolerance check, and its world point is admitted with it when only the header is in the stream. The code has no limit of fifteen bits for the bridge and loop tables. The summary line counts the admitted records, the indices examined, the duplicate owners, and the records that used a wide pointer. It also counts why the loop ring of the faces without loops did not close: a coedge absent from the tables, a coedge of another loop, a cycle, or a null next reference. The first cause wins.

Round 11. Round 10 on the real files confirmed the wide-pointer reading: 483,095 admitted records used a wide pointer, the faces of converted documents rose from 425,471 to 481,401, and the first-loop references that name nothing fell from 26,379 to 791. One document is now refused for `coedge_pairing`, and six more show it as a warning. A coedge pair that names each other as twin must lie on one edge and have opposite senses, so the second pass admits a coedge only if the twin it names agrees. The decoder admits a vertex together with its world point, read with the wide-pointer rule, because round 10 admitted none of the 8,892 vertex references: the point was always missing. A body hierarchy that the admitted bridges break falls back to the bridges of the scan. Of the 16,502 faces without loops, 11,292 had loops that were all dropped later because an edge could not be emitted; the summary counts these apart from the faces whose loop walk found no loop. The loss `topology.face-unclaimed` counts faces that no body claims. In 44 documents the likely cause is a bridge admitted by reference that has no typed face node; the summary line `summary2` counts them. The rescore shows whether the other unclaimed faces exist.

Round 12. Round 11 lost 2,806 faces in 8 documents against round 10, and the loss `topology.face-unclaimed` rose from 45 to 53 documents. The cause is the rule that keeps an admitted bridge without a typed face node out of the body hierarchy: the hierarchy no longer names the bridge, and the decoder drops every face that no body names. A hosted decode now binds such a face to the group and the shell of the faces that share an edge with it, if all bound neighbours agree. The summary line `summary2` states the count in the field `adopted_faces`. A face that shares no edge with a bound face stays unclaimed. The loop field of the coedge (725 rings) and the owner field of the bridge (3,570 references) are not changed, because the counts do not state which field holds the loop.

Round 13. The coordinator asked for a check of the 27 complete models whose stored volume (mass-property code 56, flag 1) is at least 20 times the computed volume, because a model that is complete and misses a body is the worst result. The check found 42 such models on the whole share with the final decoder. All 42 hold exactly one body in one group and one shell, with 3 to 8 faces (planes in 40 files, cylinders in 27, cones in 7, NURBS in 3). Their PARASOLID streams are small: the partition stream is 2 KB to 16 KB. No dropped edge, loop, or face is reported for them. The decoder therefore did not lose a body that the streams hold. The stored properties describe a larger solid than the stream holds. We do not know if the stored value is stale or if the rest of the geometry is outside the `PARASOLID` streams, so these models stay an open question. The checker keeps flagging them, and they must not count as agreement.

The co-occurrence of the partial reasons (3,911 partial `.par` files) is in "Next leads" 6. The junk-point change: in the deltas stream, a scan of the records found positions that read as a point with doubles near 1e-298. A real coordinate in a part is never smaller than 1e-30 m unless it is zero. A hosted scan now rejects such a record (`IMPLAUSIBLE_COORDINATE_M`). The unhosted SolidWorks path is not changed, and the sldprt goldens are not changed. The result is 14 more complete `.par` models and 81 fewer files with `face:edge_range_off_vertices`. Two experiments did not help and are not kept. A wider vertex tolerance in the checker made no file on the open corpus complete. A decode without the deltas stream, tried to learn if the deltas records are the cause of the junk, was removed from the source before the commit. The tie-break of the checker is in "Next leads" 7.

Round 14. The 763 `.par` files with `shell:region_side_undecided` and their readings (all closed readings of each body, with the stored volume) came from the checker with a trace. Most of the undecided bodies hold a band of a torus between two circles. Two readings close: the arc from the lower circle up to the upper one, and the other arc. The face sense and the coedge sense of the rings do not pick the right reading. A rule that takes the face sense of the faces reaches 42 of 220 compared models within 0.1 percent of the stored volume, and a rule that takes the reading with the most self-consistent face senses reaches 35, where the best reading of each body reaches 166. The extent of the arc does decide: of the models with a unique matching reading and a torus band, 179 prefer the arc below 180 degrees in the zone 0 to 180, and 67 prefer the complementary arc above 210 degrees; the zone 180 to 210 holds 3 and 5. Rule: take the shorter arc, and leave a band undecided when the first arc lies between 170 and 220 degrees. It completes 583 `.par` models and 8 `.psm` models, 191 of them have a stored volume, and 134 of these agree within 0.1 percent. The tie-break of round 13 completed 726 models and agreed on 34 of 241. The caps on spheres stay undecided: the truth table for them has fewer than three models per cell.

Untyped carriers. The line `summary4` states, for each untyped reference, where the compact analytic record at its index has its marker. The records at 9,900 curve references of the one-in-four sample were lines and circles with valid values whose pointers were wide (one to four wide cells among the attribute and the five references), and 2,400 surface references were planes, cylinders, cones, spheres, and tori of the same kind. The fixed offset of the scan put the marker two bytes too early for each wide pointer. The cause is the same as in round 10 for the topology records. A hosted decode now reads this form. The unresolved references are listed in "Next leads" 11. The two timeouts: the run of round 13 listed two files that timed out; the run of round 14 with the same decoder lists one, and that one times out in 150 s on three single runs (SE-19). The second timeout of round 13 did not repeat, so it came from the load of the server.

Diagnostics. Round 6 on real files left the dropped edges and loops almost unchanged, so the causes are not known. Setting the environment variable `CADMPEG_SE_DIAGNOSTICS` to any value but `0` makes the decode add report notes that start with `{"kind":`, one JSON line per dropped edge. Each line holds type codes, indices, offsets between records, flags, and counts, and no name, string, value, or coordinate. The line `drop` states, for an endpoint that did not resolve, what the stream holds at the vertex index and at the point index. The line `no_canonical` states the coedges that name an edge with no forward coedge, with their sense and the types of their loop and owners. The line `off_ring` states an edge whose canonical coedge is in no ring. The line `summary` repeats the loss counts, and the line `file` states the schema token and the `DocVersion2` code. The line `ref` states, for a reference that names no record of the type it should, the value, the nearest table keys below and above with their distance, whether the value with its low fifteen bits or its low byte, or the value shifted by up to eight, matches a key, and which streams hold a header of the expected type at that index. The field `why` of a `ref` line lists, for each stream that holds a header of the expected type at the index, the number of such headers and the checks that the strict reading fails: the marker bytes and their distance from the strict position, the class of the tolerance field (null real, plausible tolerance, zero, other), the reference layouts that parse, and for a loop whether its owner and first coedge are in the tables. The line `ring` states, for a face whose first ring did not close, the loop, the number of coedges walked, the reference at which the walk stopped with the tables and headers that hold it, and the cause. The fields `relaxed` and `wide` of `why` state whether the wide-pointer reading accepts the header and how many wide pointers it used. The line `pairs` counts the coedge pairs that name each other as twin, by how many members the second pass admitted, with the pairs whose edges differ and whose senses are equal; the line `pair` lists some of the second kind. The `admit` field of a `ref` line and the `at_admit` and `first_admit` fields of a `ring` line state what the second pass did with the index: `admitted`, `no_header`, `unreadable`, `ambiguous`, `inconsistent`, `duplicate_owner`, `no_point`, or `untried`. The line `summary3` states, for the references to curves (`what` is `curve`) and to surfaces (`surface`) that the decoder could not type, the number of references, the number of references with no header in any stream (`no_header`), the number whose index holds a header that is not of a carrier type (`other_type`), and the number per type code of the record at that index (`type_<code>`, codes 30 to 63; the partition stream wins over the deltas stream). The line `file` also states the byte length of the partition stream and of the deltas stream. The line `summary4` states, for the references to curves and to surfaces that the decoder could not type, how the compact analytic record at the index fails the parse: `m16` (marker at the strict position), `wide<n>` (marker after n wide pointers), `tripled` and `tripled_plus<n>` (deltas framing, with n extra bytes), `marker_shifted`, or `no_marker`, followed by `valid`, `frame`, `scalars`, `nonfinite`, or `short` for the values; the stream (`p` partition, `d` deltas); and, for the references with no analytic header, the distance to the nearest typed carrier key (`d_is_key`, `d_near_2`, `d_near_16`, `d_near_256`, `d_far`) and whether the index is wide. The line `keys` states, per node type, the key range, how many keys exceed `0x7fff`, how many keys equal the sequence field, the most common steps between keys in file order, and the raw header counts of both streams.

**Need.** We must know which edges stay unresolved on real documents. The counts of the new loss give the answer, and they decide whether another carrier type needs a ring rule.

Round 15. All numbers are totals from the real share, from the checker patch of round 14 plus the round 15 changes (a converter cache, a trace of all closed readings, a content-hash half bit, the transposed torus, and face traces), kept as `cad-crosscheck-d29-round15.patch` in the scratchpad and not committed. The checker is the baseline of round 14 and is not committed. "Exact" is within 0.1 percent of the stored volume, "far" more than 25 percent off, "compared" is the number of complete models with a stored volume.

Holdout of the arc rule. The files are split by a stable hash of the file content (bit 17 of the FNV-1a hash of the bytes), not by anything derived from the name. Real runs, both halves (complete, compared, exact, within 1 percent, within 5 percent, within 25 percent, far):

| rule | half | complete | compared | exact | w1 | w5 | w25 | far |
|---|---|---|---|---|---|---|---|---|
| tie-break of round 13 | h0 | 552 | 157 | 87 | 1 | 4 | 7 | 58 |
| tie-break of round 13 | h1 | 563 | 176 | 105 | 2 | 5 | 5 | 59 |
| arc rule 170/220 | h0 | 861 | 263 | 162 | 1 | 8 | 12 | 80 |
| arc rule 170/220 | h1 | 837 | 261 | 164 | 3 | 9 | 5 | 80 |
| single limit 170 | h0 | 912 | 282 | 175 | 4 | 9 | 12 | 82 |
| single limit 170 | h1 | 896 | 285 | 182 | 4 | 9 | 5 | 85 |
| single limit 180, chosen on h0 | h0 | 912 | 282 | 174 | 7 | 8 | 12 | 81 |
| single limit 180, chosen on h0 | h1 | 896 | 285 | 171 | 14 | 11 | 5 | 84 |

Only the row of the single limit 180 is blind: it was chosen on h0 and scored on h1. The limits 170 and 220 were picked in round 14 after seeing all files, including h1, so the rows of the 170/220 rule and of the single limit 170 are not held out; the split shows only that the gain is not confined to one half. The gain of the arc rule over the tie-break is of the same size in both halves (exact +75 and +59, far +22 and +21). The limits are weakly determined. A replay of the limits on the trace of the readings, chosen on h0 alone, gave 180/180, and that did not beat 170 on h1 in the real run (171 exact against 182, and 84 far against 85). Nothing was tuned further on these files. On the whole share the single limit of 170 degrees against the 170/220 rule: complete 1,808 against 1,698, compared 567 against 524, exact 357 against 326, far 167 against 160 (exact +31, far +7). The single limit of 180 gives 345 exact and 165 far.

Same models. Of the 43 models that the 170/220 rule adds to the far-off group, 36 are among the 42 compared models in which no reading of the body comes within 25 percent of the stored volume, and 7 are far off although a reading within 25 percent exists. With the single limit of 180, 39 of 48 far-off models are in that group of models without a reading within 25 percent, and 9 are not. For these 36 to 39 the rule cannot be right: no choice of side gives the stored volume. They are another defect (a missing or wrong face, or a stored volume of another body), not the side of the region.

Wide records. The check of `wide_partition_marker` found 441 of 31,258 partition wide carriers whose reference is above 16 bits; the old reading lost them because it converted every slot to `u16`. Only the attribute must fit in `u16`. The decoder now reads the pointers with the shared reader of `typed.rs` (`read_ref`) and no third copy exists. Applying the partition framing to a deltas stream would have read 21 more carriers there; these are left unread (`wide_here = hosting.rules && !is_deltas`). That these 21 were wrong was not shown, and neither was that they were right. `summary5` states the counts (partition read, new, above 16 bits, and the same for deltas).

`face:loop_not_closed_in_parameters`. The diagnosis: of the 328 files with this as the only reason, 73 hold only torus faces. Their loops wrap round the tube (meridian loops, as in pipe bends), which the integrator did not support, because it handled only loops that wrap round the axis. Swapping u and v of the torus lets the existing wrap logic handle them. With the transposed torus as a second attempt, 59 of the 73 become complete. Added to the figures of the tie-break row above (the files of this reason rerun, the other files taken from that run): complete 1,115 to 1,174, compared 333 to 345, exact 192 to 200, within 1 percent 3 to 4, within 5 percent 9 to 10, far 117 to 119, so exact +8 and far +2. It is kept in the checker. It was measured on these files only; the figures are not on top of the arc rule. A run over the whole share with the transposed torus and the single limit of 170 together was not done.

What remains of this reason: cylinder and cone faces, about 56 files with a cylinder and an unclosed u (4 turns of 0), 24 for cones, 22 cylinder with an unclosed v, 18 cones, and 88 files with several signatures. Round 16 corrects the reading of this table: the turns of plus or minus one that the trace lists are a ring that goes once round the axis, which is a valid wrap and not an error, and the cause is a closed B-spline edge (see Round 16).

`face:edge_range_off_vertices`. The 314 files hold NURBS edges whose vertices lie 10 to 100 times the tolerance off the curve; the edge is read in fit mode without a range. A NURBS tolerance 30 and 100 times wider freed no file, so the distances are larger or the failures lie elsewhere. The cause is not found.

Rule placement. The torus-band rule should stay in the checker for now. The face and coedge senses in the codec output carry no signal on torus bands (round 14), so the rule is a stand-in for a region side that the output does not state reliably; putting it into the output would hide that, and bands longer than half a turn exist. The better step is to find why the decoded senses carry no signal. It is not moved. Round 16 found why the senses carried no signal (the decoder replaced them) and measured the rule that uses the senses of the records.

Not measured: why the checker counts 335 compared models and the replay 333; the share of the loss-count change that comes from the commit alone; the wide-pointer reading in the intersection, blend, offset and spline scanners (not started, `summary4` is the measure). The one converter timeout is stable (150 s); two timeouts in the run with the r7 decoder and one in the run before were load of the server.


Round 16. The totals come from the checker patch `cad-crosscheck-d29-round16.patch` in the scratchpad (the round 15 patch plus the per-file columns of this round, the loop and sense diagnostics, and the closing-tolerance factor); it is not committed. The decoder commit is the one that keeps the face senses (below). The converter binary is the one of round 15 (`e745f3cc55`) for every number except the sense tables, which are marked.

Baseline. The whole share with the single limit of 170 degrees and the transposed torus together (`XC_ARC=1 XC_ARC_SHORT=170 XC_ARC_LONG=170 XC_TORUST=1`), `.par` documents: complete 1,870, compared 579, exact 365, within 1 percent 9, within 5 percent 19, within 25 percent 17, far 169 (h0 940 / 286 / 177 / 4 / 10 / 12 / 83, h1 930 / 293 / 188 / 5 / 9 / 5 / 86); 3,156 partial. Against the single limit 170 alone (1,808 / 567 / 357 / 167) the transposed torus adds 62 complete, 12 compared, exact +8 and far +2. The limit 170 was chosen on all files (see the holdout above), so this is not a blind number.

The far-off complete models. The 169 far-off models were classified by structure alone, with the stored density as the one feature that splits them.

- Not the cause: bodies (576 of the 579 compared models have one body; the 3 with two bodies are all far off, and in each of them one body matches the stored volume within 1 percent, so the stored value belongs to one body of two); sheets (a body that is not a solid makes the file partial, so every complete model is solid); watertight (every complete model is closed by construction: each edge is used twice and the vector area is below 1e-6); cavities (2 models subtract a cavity, both far off, and adding it instead matches in none); shells (5 models have more than one shell, 4 are far off); declared faces (none of the 579 stores display lists, so the face count cannot be compared with a declared count); body groups (every complete model has the one group `STREAM`); the deltas stream (256 files, the 136 with a zero stored density, the 43 far off with a density set, and 77 exact ones, decoded from the partition stream alone: every file keeps its class); the writing version, the modeller version, the number of saves, the first and last year of saving, and the number of variables (no split of far from exact in either population).
- Not an alternative stored number: each of the 579 files holds exactly one volume entry (ordinal 1, quantity 56, flag 1), and in none of the 169 far-off models does any other stored number match the computed volume.
- A ratio of 1 over an integer is not a class. 62 of the 126 far-off models with a zero stored density have a ratio within 1 percent of 1 over an integer, but 53 of them have an integer above 24, where some integer lies within 1 percent of any ratio. Only 9 have an integer up to 24.
- The class: the stored density. 136 of the 579 compared models (23 percent) have a stored density of zero (the reader's word for no material assigned), and 126 of the 169 far-off models are among them (75 percent): of these 136, 3 are exact, 3 within 5 percent, 4 within 25 percent, 126 far off. Of the other 443 models, 362 are exact, 9 within 1 percent, 16 within 5 percent, 13 within 25 percent, 43 far off (10 percent). For the 131 of the 136 with a non-zero stored mass, the stored mass divided by the stored volume lies in one band of 50 kg/m3 for all 131, so the stored mass is the stored volume times one fixed density and not a density of the file.
- Within the 126: the computed volume is below the stored one in 105 (83 percent), 35 of them by more than a factor of 100; the stored centre of mass lies outside the box of the computed model (with a margin of 5 percent of the diagonal) in 94 (in the other 443 models, in 5, 4 of them far off); the stored area differs from the computed one by more than 25 percent in 118 of the 136 models; 81 of the 136 have fewer than 8 faces, against 133 of the other 443.
- Is the stored set stale or does it belong to something else? Not decided by the file. The mass set is consistent with itself, and an edit history would give a stored value above the computed one (parts are cut), which is the direction of 83 percent, but 65 of the 126 have a computed volume under a tenth of the stored one, a larger change than an edit commonly makes, and 94 have a centre of mass outside the model. Both explanations fit. The decoder lost no body in the streams (round 13) and the checks above exclude the other structure. What decides it: the volume of the customer STEP export of the same document, and the feature dimension values of the file. The checker must not count these 126 as decoder errors, and they must not count as agreement either: the figures split by the stored density are the honest ones: with a density set, 443 compared, 362 exact, 43 far off.
- The other 43 far-off models (density set) have no single cause. Their computed volume is below the stored one in 25 and above it in 18. 28 of the 30 with an area more than 25 percent off are far off. The far-off share rises with the size: 7 percent at 16 to 31 faces (120 models), 17 percent at 32 to 63 (64 models), 2 of 5 at 64 to 127. The three models with two bodies and three of the five with several shells are in this group. The features tested do not split them: the far-off share is 9 to 12 percent with or without cones and with or without tori.

The face senses. The question was why the decoded face and coedge senses carry no signal on torus bands. Found: the decoder replaced them. `solve_face_orientation` rebuilds the face senses of each connected shell from the coedge senses (a rule written for the SolidWorks streams, where the coedge senses are relative to the surface normals). On a hosted Solid Edge stream the adjacent coedges have opposite senses, so the rule gives every face of the shell the sense of its first face: in the five open documents tried, every face came out forward. With the senses of the records kept (the decoder now does this on the hosted path only, `face_senses_are_solved`):

| surface | faces | face sense agrees with the side found from the edges |
|---|---|---|
| plane | 3,124 | 3,078 (98.5 percent) |
| cylinder | 1,751 | 1,217 (69.5 percent) |
| cone | 1,364 | 939 (68.8 percent) |
| torus | 1,060 | 845 (79.7 percent) |
| transposed torus | 15 | 4 |
| sphere | 6 | 2 |

The faces are those of the 365 exact models, the side found from the edges is geometric (the outward side from the closed shell), and the same faces under the overwritten senses agreed for 64 percent of the planes and 60 percent of the cylinders. The coedge senses carry the side for planes: for the 2,017 planar faces whose outer loop has no ring, the sense of the loop in the parameter plane differs from the found side in 2,014 (99.9 percent). For ring loops the relation is the opposite: the sense of the loop from the ring coedge differs from the found side in 11 percent of the 1,107 planar faces with a ring, so a ring coedge reads against the open coedges for 89 percent of them. Not the cause: the torus flip of the decoder (`orientation_reversed`, a negative major radius) never triggers in the 822 files below, and decoding without it changes nothing. The 15 transposed torus faces are too few to say anything (the table counts their sense against the reversed parameter normal). Not found: why the curved faces agree for 69 to 80 percent only and the planes for 98.5. With the senses of the records, the flags rule of the checker (the band reading follows the face sense and the ring directions) was run on the 822 files that have a torus or a sphere or an undecided shell side. The arc rule gives complete 783, compared 249, exact 174, far 54 there. The flags rule gives complete 821, compared 259, exact 30, far 107, and with the opposite handedness (region to the right of a loop seen from the outward normal, which the planes show) complete 822, compared 259, exact 53, far 78. With no rule the same files give 18 complete. So the senses of the records do not yet replace the arc rule: the flags rule loses by a wide margin, and the likely reasons are the 80 percent agreement on tori and the ring coedge reading.

Rule placement, revised. The arc rule stays in the checker. The decoder fault that hid the senses is fixed, but the senses of the curved faces and of the rings do not agree with the geometry well enough to decide a band, and no rule built on them matches the arc rule. The codec output now carries the senses of the records, which is the data the rule needs when the curved-face reading is understood.

Cylinder and cone loops. 1,047 partial files have a failing cylinder loop and 350 a failing cone loop (137 and 63 of them with this as the only reason). Most failing loops are not misclosures in u. Of the 136 failing one-segment loops on cylinders in the sole files, and of the 30 on cones, every segment is a closed B-spline edge (a ring). On cylinders it closes in u (0 or one turn round the axis, which is a valid wrap) and misses in `v` by 1 to 10 times the closing tolerance in 111 of them (by 10 to 100 times in 3), and it misses in u by 1 to 10 times (0.1 to 1.1 degrees) in 40 (17 of them in `v` too). On cones: `v` 1 to 10 times in 10, u 1 to 10 times in 20. A closed B-spline edge is a fitted curve; its ends differ by the fit tolerance. A closing tolerance ten times wider for a loop that has a B-spline edge (`XC_LTOL=10`, and the same for 100) completes 34 of the 267 sole files: complete 34 (h0 22, h1 12), compared 7, exact +4 (h0 1, h1 3), within 5 percent +1, far +2 (h0 1, h1 1). The factor 100 gives the same figures as 10. 148 of the files stay with the reason, and 60 more fail the next test, `shell:vector_area_not_zero`, and 13 `shell:orientation_conflict`: the face then closes but the shell does not. The change is small and measured on 7 models; it is kept as an option and not as the default.

Not done: the wide-pointer reading in the intersection, blend, offset, and spline scanners (task 5, `summary4`): not started. The edge range reason (`face:edge_range_off_vertices`) was not worked on again.

Not measured: whether the 126 are stale (the STEP volume decides); why the ring coedges read against the open ones; the share of the 60 closed-but-not-shell files that a better ring reading would close.


Round 17. The totals come from the checker patch `cad-crosscheck-d29-round17.patch` in the scratchpad plus one column (the computed volume of each complete model) and a curve-kind split of the edge trace (`n1`, a NURBS curve of degree one); it is not committed. The decoder is the head of round 16 unless a paragraph names the wide reading. Classes, the h0/h1 split, and the run options are those of round 16. The whole share holds 5,237 documents (`.par` 5,029, `.psm` 208); 586 have a complete model, a stored volume, and a valid error: exact 370, within 1 percent 9, within 5 percent 19, within 25 percent 17, far 171.

Customer STEP exports. A pair is a part and a STEP file (`.stp` or `.step`) with the same stem in the same directory; a stem match elsewhere counts only if it is unique on the share. The share holds 661 STEP files, and 181 parts pair by directory (157 with a stored density, 24 with a zero density); one more match elsewhere is ambiguous. The 152 pairs of the earlier rounds are not reproducible with this or any other stated rule; 181 is the count that this tooling gives. The STEP volume comes from the OCCT 7.9.3 sidecar and counts only when every solid of the STEP is valid. Of the 181, 19 have a complete model and a stored volume (13 exact, 6 far, all with a density set), 20 have a complete model and no stored volume, 142 are partial. None of the 126 far-off models with a zero density has a STEP pair, and 6 of the 45 far-off models with a density set have one. Of the 5 measurable far-off pairs, the stored volume matches the STEP and the model does not in 3, the model matches the STEP and the stored volume does not in 1, and nothing matches in 1 (0.1 percent; the same at 1 percent). The 12 measurable exact pairs agree all three ways except one, where the STEP differs from both. In every far-off pair the STEP file is later than the last save of the part (the date of the STEP header against the save history), so an older STEP does not explain a disagreement. The STEP route does not cover the zero-density question.

Stored density zero. The density field is present with a value in 2,928 `.par` (zero in 2,008, none in 93) and in 156 `.psm` (zero in 52). A zero density means no material. Of the 2,060 zero-density documents, 397 `.par` carry a stored volume; the others carry no mass-property variable. The 397 carry only 12 distinct stored sets (volume, mass, area, and centre of mass, bit for bit), and 381 of them carry one and the same set. All 126 far-off zero-density models are in that group of 381, with 5 near and 250 partial models; the 3 exact zero-density models carry sets of their own. The 131 complete models of the group have 103 distinct computed volumes, from under a tenth of the stored volume (65) to above it (26), and no complete model on the share has a volume within 1e-6 of the shared value. One stored value cannot be the volume of 103 different solids: the stored set of these documents is foreign to their geometry (a set that the documents inherited from a common origin and that was not computed again), and the far-off class of the 126 is not a decoder loss. All 381 have the first save of their history in one year. The density-set documents with a stored volume (1,456) carry 1,356 distinct sets; the largest shared group has 10 members.

The mass over the stored volume is bit-identical, 7,137.769 kg/m3, in the 386 zero-density documents with a non-zero mass. It is the ratio of the one shared set (381 documents) and of 5 documents that share its mass and volume, not a density of each document. No density-set document has this density or this ratio. The open corpus (261 documents) has no zero-density document with a stored mass, so the value does not occur there.

The custom-link field. `custom_variables` counts the records of the stream `CustomPropertyVariableInfo`: two built-in links (class `0x0200000N`, N = 1 density, 2 accuracy) and the properties that a user added. A count of two or more is the base rate (134 of 136 zero-density and 428 of 450 density-set compared files), so it does not separate far from exact. With the density held fixed the far-off share is 2 of 11 (no links), 40 of 416 (two links), and 3 of 23 (three or more) for a density set, and about 93 percent at every count for zero density. No added link holds a volume (56) or an area (13) quantity, so the stored value cannot come from linked geometry on this share.

Face senses and converted output. The checker does not read the face sense: the old decoder (every face sense solved by `solve_face_orientation`) and the head (the record senses on the hosted path) give the same classes, and the computed volumes of all 586 compared and the areas of all 1,896 complete models agree to 1e-9. The STEP export reads the face sense (`ADVANCED_FACE` same_sense). Every complete model was converted to STEP with both decoders and measured with the sidecar; the reference is the computed volume of the model.

| STEP outcome | old | head |
|---|---|---|
| valid, volume within 0.1 percent | 907 | 976 |
| valid, within 25 percent | 174 | 130 |
| valid, further off | 232 | 179 |
| not valid | 537 | 566 |
| no solid | 43 | 42 |
| no result | 3 | 3 |

86 models become valid and within 0.1 percent (68 from within 25 percent, 14 from further off, 4 from not valid); 17 leave that class (15 to within 25 percent, 1 further off, 1 not valid). 44 models become not valid; 43 of them had a valid solid with a wrong volume before. The halves agree (h0 +46/-10, h1 +40/-7). The face count, the check findings (none), and the report status do not change. The record senses give more correct output; the change stays.

Wide pointers in the intersection, blend, and offset records. The intersection (`00 26`), blend (`00 38`), and offset (`00 3c`) records have the frame of the compact analytic records, and a reference above `0x7ffe` takes two cells there too. The chart (`00 28`), terminator (`00 29`), and support-UV records write their own attribute as two cells above `0x7ffe`, which moves every later field by two bytes. The hosted scan reads these forms when the fixed reading fails. One in four partial files: untyped curve references 13,617 -> 9,554 (intersection 4,256 -> 342), untyped surface references 4,939 -> 3,850 (blend 1,301 -> 254). The whole share keeps exact 370 and far 171; no file changes status. The 178 untyped blend references of the open corpus have the fixed frame, so their blocker is elsewhere.

`face:edge_range_off_vertices`. 1,553 files have the reason, 324 as the only reason. 1,410 of them fail on edges whose curve is a NURBS curve of degree one, which is the polyline of an intersection chart (303 of the 324 sole files); the failing edge items are 15,595 on such polylines, 2,278 on other NURBS curves, 548 on lines, 500 on circles. In the open corpus, the charts of such edges have 3 or 4 points and a chordal error of 0.36 to 0.60 mm, and the vertices lie 0.02 to 0.1 mm off the polyline, against a vertex tolerance of 0.003 mm. The curve is inside the fit tolerance of its chart. The CADIR model-space curve does not carry that fit tolerance. A refinement of the chart points onto the two support surfaces would remove most of this reason.

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

### SE-20. Direction of hosted loops and of unranged arc edges

**Question.** Does every hosted stream state its loop rings and its vertex-use cells in the reverse of the order that the shared decoder reads?

**Known.** `solidedge.md` §3.3 gives the reversal that the decoder of Solid Edge applies. Observations on the 259 public `.par` files and the 2 `.psm` files: before the reversal, the loops of 34 of 34 measurable planar faces of one part ran clockwise about the normal that the face record states, and 32 of the 34 arcs of that part ran the long way round. After the reversal that part has a mesh area of 25,516.6 mm², and Solid Edge saves 25,518.3 mm² in the file (0.007 % apart). Over all files, the loops of planar faces agree with the face record in 4,455 of 4,601 cases after the reversal. The sense of a face on a cylinder, a cone, or a torus is not yet proved: 14 % of the cylinder faces still fail the OCCT face check after the reversal.

**Conflict.** The reversal makes 24 more of the 249 converted files valid and closed, and it makes 5 files that were valid and closed before invalid. In 2 of those 5 files most arcs run more than a half turn without the reversal.

The per-file difference is the sense character of the circle records. The byte after the five references of a compact curve or surface record is `+` or `-`, and the shared decoder used it only to find the record. Over the 261 public documents, 1,058 unranged open circles and 484 closed circles carry `-` (2,625 and 1,497 carry `+`). Reversing each `-` conic that has no stored range makes 213 of the 261 public documents valid and closed (175 before). The one public document with a saved volume and a converted solid then has a volume of 17,617.944 mm³ and an area of 25,518.336 mm², the saved values to the last digit (1,742.7 mm³ before). The `-` lines (1,164 unranged) need no reversal: reversing them changes no output.

On the coordinator's documents with a saved volume (a seeded sample of 600), the conic reversal raises the models that agree with the saved volume and area within 1 percent from 76 to 150; 4 that agreed before do not.

Surfaces carry the same character: 1,151 of 2,828 cylinders and 161 of 5,741 planes carry `-`. Combining it with the face sense is wrong: on the same sample it lowers the agreeing models from 150 to 136, and alone it lowers them from 76 to 75. The face sense of a hosted record already gives the side.

**Need.** We must know where the stream states the direction of an edge against its curve, so that the reversal can follow a stored value and not a convention. A reference model with a known volume per file would also show how many of the files that are valid before the reversal are valid and wrong.

### SE-21. Band of a torus face between two full circles

**Question.** Which record states the band of a torus face whose boundary is two full circles round the torus axis?

**Known.** `solidedge.md` §3.3 gives the rule that the decoder applies: the band that the loops and the face sense select must span at most half a turn of the tube, else the face sense flips. On a seeded sample of 600 of the coordinator's documents with a saved volume, the rule raises the models that agree with the saved volume and area within 1 percent from 150 to 246 (36 from an area mismatch, 51 from a volume mismatch, 9 from not valid) and loses none. Before the rule, 39 of the 40 area mismatches had a volume within 1 percent and an area 2 to 20 percent too large: the band was the rest of the tube, which adds area and only part of the tube volume. Over the 261 public documents the rule changes 13 valid models; each loses area. The same choice exists round the torus axis for a face between two tube cross-sections (a pipe elbow): 31 such faces are in the 37 public documents that fail and 2 in a sample of 60 that pass. The rule for elbows (at most half a turn round the axis) adds 1 agreeing model on the sample of 600 and 1 on the 1,853 documents with a saved volume, and loses none.

**Need.** The rule is geometric. A band of more than half a turn (a torus face that wraps most of the tube) would be cut to the wrong side. The face record, the coedge senses of the two circles, and the surface sense do not tell the bands apart on their own (SE-20: the surface sense does not combine with the face sense).

**Conflict.** Of the 1,853 documents with a saved volume, 191 agree at the pinned converter; 5 of them do not agree after SE-20 and this rule. Three are turned parts with tori whose minor radius exceeds the major radius (a self-intersecting torus); they disagree already with SE-20 and the conic reversal alone, before this rule. One is a torus cut by two planes with closed NURBS edges, and one has a sphere, a cone, and ellipse arcs.

