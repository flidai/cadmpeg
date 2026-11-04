# cadmpeg-codec-solidedge

`cadmpeg-codec-solidedge` reads the Parasolid B-rep of Solid Edge `.par`,
`.psm`, and `.asm` files into [`cadmpeg-ir`][ir]. A Solid Edge document is a
Compound File Binary container. The codec finds the Parasolid partition and
deltas pair in its `PARASOLID` storage (`STREAM0.P_B` and `STREAM0.D_B`) and
decodes it with the Parasolid B-rep decoder shared with
[`cadmpeg-codec-sldprt`][sldprt]. The stream content selects the codec; file
extensions do not.

<!-- generated: capability solidedge -->

Support: L1 ([ladder](https://github.com/cadmpeg/cadmpeg/blob/main/docs/format-support.md#siemens-solid-edge-par-psm-and-asm)).

<!-- /generated: capability solidedge -->

The codec is read-only. It transfers the B-rep only: feature history, sketches,
document properties, materials, and assembly structure are not read.

## Install

```sh
cargo add cadmpeg-codec-solidedge cadmpeg-ir
```

## Decode a part

```rust,no_run
use std::fs::File;

use cadmpeg_codec_solidedge::SolidEdgeCodec;
use cadmpeg_ir::{Codec, DecodeOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = File::open("part.par")?;
    let decoded = SolidEdgeCodec.decode(&mut input, &DecodeOptions::default())?;

    println!(
        "{} bodies, {} faces, geometry_transferred={}",
        decoded.ir().model.bodies.len(),
        decoded.ir().model.faces.len(),
        decoded.report().geometry_transferred(),
    );
    for loss in &decoded.report().losses {
        eprintln!("{:?}: {}", loss.severity, loss.message);
    }
    Ok(())
}
```

Read the report losses before trusting the model. A document without a
`PARASOLID` storage, without a partition stream, or whose pair decodes no
geometry returns a container-only result with a typed loss and no model.
Detection is content-based and needs the `PARASOLID` storage, so most
assemblies, which carry no B-rep of their own, are not detected. A forced
decode of such a file reports `container.no-parasolid-storage`.

Transferred entity identities keep the `sldprt:brep` namespaces of the shared
Parasolid decoder, and its losses keep the `sldprt` loss namespace.

## Documentation

- [API documentation][docs]
- [Format support][support]
- [Solid Edge format notes][spec]
- [Architecture and crate map][architecture]
- [Clean-room and legal policy][legal]
- [Repository][repo]

Requires Rust 1.88 or later. Licensed under Apache-2.0.

Siemens, Solid Edge, Parasolid, and other product names are trademarks of their
respective owners. cadmpeg uses them only to identify the file formats this
codec targets and is not affiliated with, endorsed by, or sponsored by any CAD
vendor. See the [clean-room and legal policy][legal].

[architecture]: https://github.com/cadmpeg/cadmpeg/blob/main/docs/architecture.md
[docs]: https://docs.rs/cadmpeg-codec-solidedge
[ir]: https://docs.rs/cadmpeg-ir
[legal]: https://github.com/cadmpeg/cadmpeg/blob/main/LEGAL.md
[repo]: https://github.com/cadmpeg/cadmpeg
[sldprt]: https://docs.rs/cadmpeg-codec-sldprt
[spec]: https://github.com/cadmpeg/cadmpeg/blob/main/docs/formats/solidedge.md
[support]: https://github.com/cadmpeg/cadmpeg/blob/main/docs/format-support.md
