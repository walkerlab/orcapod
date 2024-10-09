use orcapod::model::{Annotation, Pod};
use orcapod::store::{LocalFileStore, OrcaStore};
use regex::Regex;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

#[test]
fn pod() -> Result<(), Box<dyn Error>> {
    let fs = LocalFileStore {
        location: PathBuf::from("./test_store"),
    };
    let pod = Pod::new(
        Annotation {
            name: String::from("style-transfer"),
            description: String::from("This is an example pod."),
            version: String::from("0.67.0"),
        },
        String::from("tail -f /dev/null"),
        String::from("zenmldocker/zenml-server:0.67.0"),
        BTreeMap::from([
            (
                String::from("painting"),
                PathBuf::from("/input/painting.png"),
            ),
            (String::from("image"), PathBuf::from("/input/image.png")),
        ]),
        PathBuf::from("/output"),
        BTreeMap::from([(String::from("styled"), PathBuf::from("./styled.png"))]),
        0.25,                   // 250 millicores as frac cores
        (2 as u64) * (1 << 30), // 2GiB in bytes
        String::from("https://github.com/zenml-io/zenml/tree/0.67.0"),
    )?;

    match fs.save_pod(&pod) {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e)
        }
    };
    println!("{:?}", fs.list_pod());
    // println!("{:?}", fs);
    Ok(())
}

#[test]
fn path_loop_benchmark() {
    let mut paths: Vec<PathBuf> = vec![];
    let root = "test_store/annotation/pod";
    let pod_names = ["test", "something"];
    // let pod_name_ids: Vec<i32> = (0..24_000).collect(); // ~11.13s / ~11.13s
    let pod_name_ids: Vec<i32> = (0..3).collect();
    let hashes = ["a1b2c3", "d4e5f6"];
    let versions = ["0.1.0", "0.2.0"];
    for pod_name in pod_names {
        for pod_name_id in &pod_name_ids {
            for hash in hashes {
                for version in versions {
                    paths.push(PathBuf::from(format!(
                        "{root}/{pod_name}-{pod_name_id}/{hash}-{version}.yaml"
                    )));
                }
            }
        }
    }
    // println!("paths: {:?}, len: {}", paths, paths.len());

    let re = Regex::new(r"^.*\/(?<name>[0-9a-zA-Z\-]+)\/(?<hash>[0-9a-f]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$").unwrap();

    // let now = Instant::now();
    // {
    //     let mut names: Vec<String> = vec![];
    //     let mut hashes: Vec<String> = vec![];
    //     let mut versions: Vec<String> = vec![];
    //     for p in paths {
    //         let path_string = &p.display().to_string();
    //         let caps = re.captures(path_string).unwrap();
    //         names.push(caps["name"].to_string());
    //         hashes.push(caps["hash"].to_string());
    //         versions.push(caps["version"].to_string());
    //     }

    //     // for (name, (hash, version)) in names.iter().zip(hashes.iter().zip(versions.iter())) {
    //     //     println!("name: {}, hash: {}, version: {}", name, hash, version);
    //     // }
    // }
    // let elapsed = now.elapsed();
    // println!("Elapsed: {:.2?}", elapsed);

    let now = Instant::now();
    {
        let (names, (hashes, versions)): (Vec<String>, (Vec<String>, Vec<String>)) = paths
            .iter()
            .map(|p| {
                let path_string = &p.display().to_string();
                let cap = re.captures(path_string).unwrap();
                (
                    cap["name"].to_string(),
                    (cap["hash"].to_string(), cap["version"].to_string()),
                )
            })
            .unzip();

        // for (name, (hash, version)) in names.iter().zip(hashes.iter().zip(versions.iter())) {
        //     println!("name: {}, hash: {}, version: {}", name, hash, version);
        // }
    }
    let elapsed = now.elapsed();
    println!("Elapsed: {:.2?}", elapsed);
}
