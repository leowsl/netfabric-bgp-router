use clickhouse::Client;

pub struct DBConnectionArgs {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
}

pub trait DBConnection {
    fn new(args: DBConnectionArgs) -> Self;
    fn get_client(&self) -> &Client;
}

pub struct ClickhouseConnection {
    pub client: Client,
}
impl DBConnection for ClickhouseConnection {
    fn new(args: DBConnectionArgs) -> Self {
        let client = Client::default()
            .with_url(format!("http://{}:{}", args.host, args.port))
            .with_user(args.username)
            .with_password(args.password)
            .with_database(args.database);
        Self { client }
    }

    fn get_client(&self) -> &Client {
        &self.client
    }
}
