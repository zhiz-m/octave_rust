use std::{collections::BTreeMap, fs};

use anyhow::Context;
use serenity::prelude::TypeMapKey;

type Data = BTreeMap<String, String>;

pub struct Db {
    path: String,
    data: Data,
}

fn save_data(path: &str, data: &Data) -> anyhow::Result<()> {
    let data = serde_json::to_string_pretty(data)?;
    fs::write(path, data).context("failed to write db")
}

fn load_or_init_data(path: &str) -> anyhow::Result<Data> {
    let data_exists = fs::exists(path).context("failed to check existence of file")?;
    match data_exists {
        true => {
            let data = fs::read_to_string(path).context("failed to load db")?;
            serde_json::from_str(&data).context("failed to deserialize data from db")
        }
        false => {
            let data = BTreeMap::new();
            save_data(path, &data)?;
            Ok(data)
        }
    }
}

impl Db {
    pub fn new(path: String) -> anyhow::Result<Self> {
        let data = load_or_init_data(&path)?;
        Ok(Self { data, path })
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    pub fn insert_and_flush(&mut self, key: String, value: String) -> anyhow::Result<()> {
        self.data.insert(key, value);
        save_data(&self.path, &self.data)
    }
}

impl TypeMapKey for Db {
    type Value = Self;
}
