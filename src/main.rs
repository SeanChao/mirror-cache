#[tokio::main]
async fn main() {
    let api = filters::root();
    warp::serve(api).run(([127, 0, 0, 1], 9000)).await;
}

mod filters {
    use super::handlers;
    use warp::Filter;

    pub fn root() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        pypi_index().or(pypi_packages())
    }

    // GET /pypi/web/simple/:string
    fn pypi_index() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "web" / "simple" / String).and_then(handlers::get_pypi_index)
    }

    // GET /pypi/package/:string/:string/:string/:string
    fn pypi_packages() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("pypi" / "packages" / String / String / String / String)
            .and_then(handlers::get_pypi_pkg)
    }
}

mod handlers {
    use reqwest::ClientBuilder;
    use warp::{http::Response, Rejection};

    pub async fn get_pypi_index(path: String) -> Result<impl warp::Reply, Rejection> {
        let upstream = format!("https://pypi.org/simple/{}", path);
        let client = ClientBuilder::new().build().unwrap();
        let resp = client.get(&upstream).send().await;
        match resp {
            Ok(response) => Ok(Response::builder()
                .header("content-type", "text/html")
                .body(response.text().await.unwrap().replace(
                    "https://files.pythonhosted.org/packages",
                    format!("http://localhost:9000/pypi/packages").as_str(),
                ))),
            Err(err) => {
                println!("{:?}", err);
                Err(warp::reject::reject())
            }
        }
    }

    pub async fn get_pypi_pkg(
        path: String,
        path2: String,
        path3: String,
        path4: String,
    ) -> Result<impl warp::Reply, Rejection> {
        let path = format!("{}/{}/{}/{}", path, path2, path3, path4);
        println!("pypi_pkg: {}", path);
        let upstream = format!("https://files.pythonhosted.org/packages/{}", path);
        let client = ClientBuilder::new().build().unwrap();
        println!("GET {}", &upstream);
        let resp = client.get(&upstream).send().await;
        match resp {
            Ok(response) => {
                println!("fetched {}", response.content_length().unwrap());
                Ok(Response::builder().body(response.bytes().await.unwrap()))
            }
            Err(err) => {
                println!("{:?}", err);
                Err(warp::reject::reject())
            }
        }
    }
}
