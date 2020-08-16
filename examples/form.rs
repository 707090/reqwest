#[tokio::main]
async fn main() {
    reqwest::RequestBuilder::post("http://www.baidu.com")
        .form(&[("one", "1")])
        .send_with(&reqwest::Client::new())
        .await
        .unwrap();
}
