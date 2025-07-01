A tiny Outlook Email Message (.msg) reader
=======================================

A tiny reader for .msg files.  

### Usage Example

```rust
use tiny_msg::Email;

fn main() {
    let email = Email::from_path("sample/sample1.msg");

    dbg!(&email.from);
    dbg!(&email.to);
    dbg!(&email.cc);
    dbg!(&email.sent_date);
    dbg!(&email.subject);
    dbg!(&email.body);
}
```

More examples in the `examples` directory.
