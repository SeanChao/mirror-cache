use crate::error::Error::*;
use crate::error::Result;
use reqwest::ClientBuilder;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use warp::hyper::body::Bytes;

type Responder<T> = oneshot::Sender<T>;

#[derive(Debug)]
pub struct DownloadTask {
    pub url: String,
    pub resp: Responder<Result<Bytes>>,
}

// TODO: multi-thread download
pub async fn task_recv(mut rx: mpsc::Receiver<DownloadTask>) {
    let client = ClientBuilder::new().build().unwrap();
    let mut task_set: HashSet<String> = HashSet::new();
    while let Some(task) = rx.recv().await {
        println!("-> [len={}] recv: {}", task_set.len(), &task.url);
        let url = task.url;
        if task_set.contains(&url) {
            println!("\tignored {} because it's in the queue", &url);
            continue;
        }
        task_set.insert(url.clone());
        println!("\tGET {}", &url);
        let resp = client.get(&url).send().await;
        match resp {
            Ok(response) => {
                let _ = task.resp.send(Ok(response.bytes().await.unwrap()));
            }
            Err(e) => {
                let _ = task.resp.send(Err(RequestError(e)));
            }
        }
        task_set.remove(&url);
    }
}
