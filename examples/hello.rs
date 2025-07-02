use tiny_msg::Email;

fn main() {
    let email = Email::from_path("sample/sample1.msg");
    // basic headers
    println!("From: {:?}", email.from.unwrap());
    println!("To: {:?}", email.to);
    println!("Cc: {:?}", email.cc);
    println!("Subject: {}", email.subject.unwrap());
    println!();
    // email body
    println!("Body starts with: {:?}", &email.body.unwrap()[..50]);
    println!();
    // attachments
    println!(
        "Attachments: {:?}",
        email
            .attachments
            .iter()
            .map(|a| a.name.clone())
            .collect::<Vec<_>>()
    );
    println!();
    // embedded messages
    println!(
        "Embedded Messages: {:?}",
        email
            .embedded_messages
            .iter()
            .map(|m| m.subject.as_ref().unwrap())
            .collect::<Vec<_>>()
    );
    println!();
    // OUTPUT: 
    // From: ("spoj", "spoj@example.com")
    // To: [("john", "john@example.com")]
    // Cc: [("karen", "karen@example.com")]
    // Subject: Weekend plan

    // Body starts with: "<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html"

    // Attachments: ["image001.png", "image002.png"]

    // Embedded Messages: ["Your flight itinerary"]
}
