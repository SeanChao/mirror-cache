use crate::error::Error::*;
use crate::error::Result;
use futures::lock::Mutex;
use reqwest::ClientBuilder;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::sync::watch::Receiver;
use warp::hyper::body::Bytes;

type Responder<T> = oneshot::Sender<T>;

#[derive(Debug)]
pub struct DownloadTask {
    pub url: String,
    pub resp: Responder<Result<Bytes>>,
}
impl PartialEq for DownloadTask {
    fn eq(&self, other: &DownloadTask) -> bool {
        self.url == other.url
    }
}

impl Eq for DownloadTask {}

pub async fn task_recv(mut rx: mpsc::Receiver<DownloadTask>) {
    let task_set: HashSet<String> = HashSet::new();
    let task_rx_map: HashMap<String, Receiver<Option<Bytes>>> = HashMap::new();
    let task_set_sync = Arc::new(Mutex::new(task_set));
    let mut task_rx_map_sync = Arc::new(Mutex::new(task_rx_map));
    while let Some(task) = rx.recv().await {
        let taskset2 = Arc::clone(&task_set_sync);
        let task_rx_map_sync_clone = Arc::clone(&mut task_rx_map_sync);
        tokio::spawn(async move {
            let url = task.url;
            let client_clone = ClientBuilder::new().build().unwrap();
            let mut tx = None;
            {
                let mut tasks = taskset2.lock().await;
                if !tasks.contains(&url) {
                    tasks.insert(url.clone());
                    let (chan_tx, rx) = watch::channel(None);
                    // create channel
                    tx = Some(chan_tx);
                    task_rx_map_sync_clone.lock().await.insert(url.clone(), rx);
                }
            }
            // lock released, start downloading the package
            if !tx.is_none() {
                // multiple same tasks may be sent concurrently, only one thread whose tx is not None starts the download
                println!("[Task] downloading {}", &url);
                match client_clone.get(&url).send().await {
                    Ok(response) => {
                        tx.unwrap()
                            .send(Some(response.bytes().await.unwrap()))
                            .unwrap();
                    }
                    Err(e) => {
                        eprintln!("[Task] error downloading {}: {}", &url, e);
                        tx.unwrap().send(None).unwrap();
                    }
                }
                let mut tasks = taskset2.lock().await;
                tasks.remove(&url);
                println!("[Task] Finished downloading {}", &url);
            }
            {
                if let Some(rx) = task_rx_map_sync_clone.lock().await.get(&url) {
                    if let Some(result) = rx.borrow().clone() {
                        let _ = task.resp.send(Ok(result));
                    } else {
                        let _ = task
                            .resp
                            .send(Err(DownloadTaskError(String::from("result is none"))));
                    }
                }
            }
        });
    }
}
