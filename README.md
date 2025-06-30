A tiny Outlook Email Message (.msg) reader
=======================================

A tiny reader for .msg files.  

### Usage Example

```rust
use std::path::Path;
use tiny_msg::MsgReader;

fn main() {
    let mut cfb = cfb::open("/path/to/your.msg").unwrap();
    let mut reader = MsgReader::new(&mut cfb, Path::new("/"));

    dbg!(&reader.from());
    dbg!(&reader.to());
    dbg!(&reader.cc());
    dbg!(&reader.sent_date());
    dbg!(&reader.subject());
    dbg!(&reader.body());
}
