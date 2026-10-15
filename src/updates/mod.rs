//! Signed, opt-in updates. The independent worker survives stopping MX.
pub mod api;
mod config;
mod health;
mod package;
mod store;
mod worker;

pub(crate) use config::UpdateConfig;
use std::sync::LazyLock;
static CONFIG: LazyLock<Result<Option<UpdateConfig>, String>> =
    LazyLock::new(config::server_config);
pub fn configuration() -> Result<Option<&'static UpdateConfig>, String> {
    CONFIG.as_ref().map(Option::as_ref).map_err(Clone::clone)
}
pub fn validate_startup() -> Result<(), String> {
    configuration().map(|_| ())
}
pub fn in_maintenance() -> bool {
    match configuration() {
        Ok(Some(config)) => config.root.join("maintenance.json").exists(),
        Ok(None) => false,
        Err(_) => true,
    }
}

pub async fn cli(args: Vec<String>) -> Option<Result<(), String>> {
    let command = args.first()?.as_str();
    if !matches!(
        command,
        "--update-worker" | "--update-keygen" | "--update-package"
    ) {
        return None;
    }
    Some(match (command,args.as_slice()) {
        ("--update-worker",[_,flag,path]) if flag=="--config" => match UpdateConfig::load(path.as_ref()) {
            Ok(config)=>worker::run(&config).await,
            Err(error)=>Err(error),
        },
        ("--update-keygen",[_,path])=>package::keygen(path.as_ref()),
        ("--update-package",[_,source,key,output,channel])=>package::build(source.as_ref(),key.as_ref(),output.as_ref(),channel).await,
        _=>Err("Usage: --update-worker --config FILE | --update-keygen PRIVATE_KEY_FILE | --update-package RELEASE_DIRECTORY PRIVATE_KEY_FILE OUTPUT.mxupdate CHANNEL".into()),
    })
}

#[cfg(test)]
mod tests;
