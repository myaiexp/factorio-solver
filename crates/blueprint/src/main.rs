use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: factorio-blueprint <command> <blueprint-string>");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  decode     Decode a blueprint string and print pretty JSON");
        eprintln!("  roundtrip  Decode then re-encode a blueprint string");
        process::exit(1);
    }

    let command = &args[1];
    let blueprint_string = &args[2];

    match command.as_str() {
        "decode" => match factorio_blueprint::decode_to_json(blueprint_string) {
            Ok(json) => {
                let value: serde_json::Value =
                    serde_json::from_str(&json).expect("decoded JSON should be valid");
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        "roundtrip" => match factorio_blueprint::decode(blueprint_string) {
            Ok(data) => match factorio_blueprint::encode(&data) {
                Ok(encoded) => println!("{encoded}"),
                Err(e) => {
                    eprintln!("Error encoding: {e}");
                    process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("Error decoding: {e}");
                process::exit(1);
            }
        },
        other => {
            eprintln!("Unknown command: {other}");
            eprintln!("Use 'decode' or 'roundtrip'");
            process::exit(1);
        }
    }
}
