use std::path::Path;

use reth_primitives::Address;
use serde::Deserialize;

pub const CONFIG_PREFIX: &str = "OP_PROPOSER";

#[derive(Debug, Clone, Deserialize)]
pub struct OpProposerConfig {
    pub l2_output_db: String,
    pub l1_rpc: String,
    pub rollup_rpc: String,
    pub proposer_type: ProposerType,
    pub l2_to_l1_message_passer: Address,
    pub l2_output_oracle: Address,
    pub proposer_private_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ProposerType {
    DisputeGameFactory(DisputeGameFactoryConfig),
    L2OutputOracle,
}

impl OpProposerConfig {
    pub fn load(config_path: Option<&Path>) -> eyre::Result<Self> {
        let mut settings = config::Config::builder();

        if let Some(path) = config_path {
            settings = settings.add_source(config::File::from(path).required(true));
        }

        let settings = settings
            .add_source(
                config::Environment::with_prefix(CONFIG_PREFIX).separator("__").try_parsing(true),
            )
            .build()?;

        let config = settings.try_deserialize::<Self>()?;

        Ok(config)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisputeGameFactoryConfig {
    pub dispute_game_factory: Address,
    pub game_type: u32,
}
