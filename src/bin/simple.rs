// cargo run --bin simple
#![expect(missing_docs, reason = "debug")]

use futures_util::future::{join_all, try_join_all};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs,
    io,
    // process::Command,
    result,
    sync::Arc,
    thread::sleep as sync_sleep,
    time::{self, Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::{
    runtime::Runtime,
    spawn,
    sync::{Mutex, mpsc, watch},
    task::{self, JoinSet},
    time::sleep as async_sleep,
};
// use zenoh;

#[derive(Error, Debug)]
enum Kind {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Watch(#[from] watch::error::RecvError),
    #[error(transparent)]
    SystemTime(#[from] time::SystemTimeError),
    #[error(transparent)]
    TokioTaskJoin(#[from] task::JoinError),
    #[error(transparent)]
    TokioSendString(#[from] mpsc::error::SendError<String>),
}

#[derive(Error, Debug)]
#[error(transparent)]
struct CustomError(#[from] Kind);

impl From<io::Error> for CustomError {
    fn from(error: io::Error) -> Self {
        Self(Kind::Io(error))
    }
}

// impl From<&Kind> for CustomError {
//     fn from(error: &Kind) -> Self {
//         Self(error)
//     }
// }

impl From<watch::error::RecvError> for CustomError {
    fn from(error: watch::error::RecvError) -> Self {
        Self(Kind::Watch(error))
    }
}

impl From<time::SystemTimeError> for CustomError {
    fn from(error: time::SystemTimeError) -> Self {
        Self(Kind::SystemTime(error))
    }
}

impl From<task::JoinError> for CustomError {
    fn from(error: task::JoinError) -> Self {
        Self(Kind::TokioTaskJoin(error))
    }
}

impl From<mpsc::error::SendError<String>> for CustomError {
    fn from(error: mpsc::error::SendError<String>) -> Self {
        Self(Kind::TokioSendString(error))
    }
}

type Result<T, E = CustomError> = result::Result<T, E>;

fn procedural_finite(files: &[String]) -> Result<Vec<String>> {
    files
        .iter()
        .map(|file| {
            println!("processing: {file}");
            let content = fs::read_to_string(file)?;
            sync_sleep(Duration::from_secs(5));
            Ok::<_, CustomError>(content)
        })
        .collect::<Result<Vec<_>>>()
}

fn concurrent_finite(files: &[String]) -> Result<Vec<String>> {
    let runtime = Runtime::new()?;

    runtime.block_on(async {
        try_join_all(files.iter().map(|file| {
            let inner_file = file.clone();
            async move {
                println!("processing: {inner_file}");
                let content = fs::read_to_string(&inner_file)?;
                async_sleep(Duration::from_secs(5)).await;
                Ok::<_, CustomError>(content)
            }
        }))
        .await
    })
}

fn concurrent_finite_spawner(files: &[String]) -> Result<()> {
    let runtime = Runtime::new()?;

    for file in files {
        runtime.spawn({
            let inner_file = file.clone();
            async move {
                println!("processing: {inner_file}");
                let content = fs::read_to_string(inner_file)?;
                async_sleep(Duration::from_secs(5)).await;
                Ok::<(), CustomError>(())
            }
        });
    }
    sync_sleep(Duration::from_secs(20));
    Ok(())
}

#[expect(clippy::excessive_nesting, clippy::unwrap_used, reason = "debug")]
fn concurrent_indefinite(files: &Vec<String>) -> Result<()> {
    let runtime = Runtime::new()?;
    runtime.block_on(async {
        let (request_tx, mut request_rx) = mpsc::channel(10);
        let (response_tx, mut response_rx) = mpsc::channel(10);

        for file in files {
            request_tx.send(file.to_owned()).await?;
        }

        let mut set = JoinSet::new();
        set.spawn(async move {
            while let Some(file_path) = request_rx.recv().await {
                spawn({
                    let inner_response_tx = response_tx.clone();
                    async move {
                        println!("processing: {file_path}");
                        let content = fs::read_to_string(&file_path).map_err(CustomError::from);
                        async_sleep(Duration::from_secs(5)).await;
                        inner_response_tx.send(content).await
                    }
                });
            }
            Ok(())
        });
        set.spawn(async move {
            while let Some(content) = response_rx.recv().await {
                println!("{}", content?);
            }
            Ok(())
        });
        set.join_next().await.unwrap()?
    })
}

// #[expect(clippy::excessive_nesting, clippy::unwrap_used, reason = "debug")]
// fn concurrent_indefinite_zenoh(files: &Vec<String>) -> Result<()> {
//     let runtime = Runtime::new()?;
//     runtime.block_on(async {
//         let client = zenoh::open(zenoh::Config::default()).await.unwrap();

//         let mut set = JoinSet::new();
//         set.spawn(async move {
//             while let Ok(sample) = client
//                 .declare_subscriber("request")
//                 .await
//                 .unwrap()
//                 .recv_async()
//                 .await
//             {
//                 spawn({
//                     async move {
//                         let file_path = sample
//                             .payload()
//                             .try_to_string()
//                             .unwrap_or_else(|error| error.to_string().into())
//                             .to_string();
//                         println!("processing: {file_path}");
//                         let content = fs::read_to_string(&file_path).map_err(CustomError::from);
//                         async_sleep(Duration::from_secs(5)).await;
//                         client.put("result", content).await.unwrap()
//                     }
//                 });
//             }
//             Ok(())
//         });
//         set.spawn(async move {
//             while let Ok(sample) = client
//                 .declare_subscriber("result")
//                 .await
//                 .unwrap()
//                 .recv_async()
//                 .await
//             {
//                 let content =
//                     serde_json::from_slice::<Result<String>>(&sample.payload().to_bytes()).unwrap();
//                 println!("{}", content?);
//             }
//             Ok(())
//         });

//         for file in files {
//             client.put("request", file.to_owned()).await.unwrap();
//         }

//         // sync_sleep(Duration::from_secs(10));

//         set.join_next().await.unwrap()?
//     })
// }

fn main() -> Result<()> {
    // find some files
    // vec!["security_notice.txt", ".profile", ".bash_logout", ".gitignore"]
    // let response = String::from_utf8(
    //     Command::new("ls")
    //         .arg("-lah")
    //         .output()
    //         .expect("failed to execute process")
    //         .stdout,
    // )
    // .unwrap();
    // println!("{}", response);

    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let files = vec![
        ".gitignore".to_owned(),
        "security_notice.txt".to_owned(),
        "LICENSE".to_owned(),
    ];

    // println!("contents: {:#?}", procedural_finite(&files)?);
    // println!("contents: {:#?}", concurrent_finite(&files)?);
    // println!("contents: {:#?}", concurrent_finite_spawner(&files)?);
    println!("contents: {:#?}", concurrent_indefinite(&files)?);
    // println!("contents: {:#?}", concurrent_indefinite_zenoh(&files)?);

    println!(
        "started: {started}, duration: {}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() - started
    );
    Ok(())
}
