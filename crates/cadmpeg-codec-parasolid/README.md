# cadmpeg-codec-parasolid

`cadmpeg-codec-parasolid` reads the Parasolid B-rep of a standalone transmit
file (`.x_b`) into [`cadmpeg-ir`][ir]. A transmit file has a text header that
starts with `**`, followed by the transmit data. The codec reads the header,
checks the encoding of the data, and decodes the binary data with the Parasolid
B-rep decoder shared with [`cadmpeg-codec-sldprt`][sldprt]. The content selects
the codec; file extensions do not.

<!-- generated: capability parasolid -->

Support: L1 ([ladder](https://github.com/cadmpeg/cadmpeg/blob/main/docs/format-support.md#parasolid-transmit-xb-and-xt-files)).

<!-- /generated: capability parasolid -->

The codec is read-only. A transmit file whose header declares `FORMAT=text`
(`.x_t`) is identified and refused with a typed result: the text encoding of the
transmit data is not read.

## Install

```sh
cargo add cadmpeg-codec-parasolid cadmpeg-ir
```

## Decode a part

```rust,no_run
use std::fs::File;

use cadmpeg_codec_parasolid::ParasolidCodec;
use cadmpeg_ir::{Codec, DecodeOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = File::open("part.x_b")?;
    let decoded = ParasolidCodec.decode(&mut input, &DecodeOptions::default())?;

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

Read the report losses before trusting the model. Transferred entity identities
keep the `sldprt:brep` namespaces of the shared Parasolid decoder, and its
losses keep the `sldprt` loss namespace.

## Documentation

- [API documentation][docs]
- [Format support][support]
- [Parasolid transmit file notes][spec]
- [Architecture and crate map][architecture]
- [Clean-room and legal policy][legal]
- [Repository][repo]

Requires Rust 1.88 or later. Licensed under Apache-2.0.

Siemens, Parasolid, and other product names are trademarks of their respective
owners. cadmpeg uses them only to identify the file formats this codec targets
and is not affiliated with, endorsed by, or sponsored by any CAD vendor. See the
[clean-room and legal policy][legal].

[architecture]: https://github.com/cadmpeg/cadmpeg/blob/main/docs/architecture.md
[docs]: https://docs.rs/cadmpeg-codec-parasolid
[ir]: https://docs.rs/cadmpeg-ir
[legal]: https://github.com/cadmpeg/cadmpeg/blob/main/LEGAL.md
[repo]: https://github.com/cadmpeg/cadmpeg
[sldprt]: https://docs.rs/cadmpeg-codec-sldprt
[spec]: https://github.com/cadmpeg/cadmpeg/blob/main/docs/formats/parasolid.md
[support]: https://github.com/cadmpeg/cadmpeg/blob/main/docs/format-support.md
