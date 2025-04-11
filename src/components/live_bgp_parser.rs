use reqwest::get;
use futures_util::StreamExt;

static RIS_STREAM_URL: &str = "https://ris-live.ripe.net/v1/stream/?format=json&client=Netfabric-Test";

pub async fn start_stream() -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = get(RIS_STREAM_URL).await?.bytes_stream();

    while let Some(item) = stream.next().await {
        println!("Chunk: {:?}", item?);
    }

    Ok(())
}