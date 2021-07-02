use reqwest::ClientBuilder;
use warp::{http::Response, path::FullPath, Filter, Rejection};

#[tokio::main]
async fn main() {
    // GET /pypi/web/simple
    let pypi_index_routes = warp::path!("pypi" / "web" / "simple" / String).and_then(pypi_index);
    // GET /pypi/package/path
    let pypi_pkg_route =
        warp::path!("pypi" / "packages" / String / String / String / String).and_then(pypi_pkg);
    let fallback = warp::path::full()
        .map(move |path: FullPath| format!("fallback: {}", path.as_str().to_string()));
    let routes = warp::get().and(pypi_index_routes.or(pypi_pkg_route).or(fallback));

    warp::serve(routes).run(([127, 0, 0, 1], 9000)).await;
}

async fn pypi_index(path: String) -> Result<impl warp::Reply, Rejection> {
    let upstream = format!("https://pypi.org/simple/{}", path);
    let client = ClientBuilder::new().build().unwrap();
    let resp = client.get(&upstream).send().await;
    match resp {
        Ok(response) => Ok(Response::builder()
            .header("content-type", "text/html")
            .body(response.text().await.unwrap())),
        Err(err) => {
            println!("{:?}", err);
            Err(warp::reject::reject())
        }
    }
}

async fn pypi_pkg(
    path: String,
    path2: String,
    path3: String,
    path4: String,
) -> Result<impl warp::Reply, Rejection> {
    let path = format!("{}/{}/{}/{}", path, path2, path3, path4);
    println!("pypi_pkg: {}", path);
    let upstream = format!("https://files.pythonhosted.org/packages/{}", path);
    let client = ClientBuilder::new().build().unwrap();
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
