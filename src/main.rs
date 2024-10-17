use regex::Regex;

fn main() {
    let str = String::from("orca-data-test/annotation/pod/Style Transfer Test Pod/c7a81f25b4ab5afd60584b6ca79893e2d963bda64ccb69178a26abd3a29bf0c2-0.0.0.yaml");

    let re = Regex::new(
        r"\/(?<name>[0-9a-zA-Z\- ]+)\/(?<hash>[0-9a-f]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$",
    )
    .unwrap();

    println!("{:?}", str);
    println!("{:?}", re.captures(&str).unwrap());
}
