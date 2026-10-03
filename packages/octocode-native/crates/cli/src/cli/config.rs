use super::{emit_error, write_json};
use octocode_native::runtime::ToolRuntime;
use serde_json::json;
use std::io::{self, Read};

pub fn show_path(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let path = runtime.inspect_config().global_env_path;
    if json_out {
        write_json(&json!({"path": path, "exists": path.is_file()}), true)
    } else {
        println!("{}", path.display());
        0
    }
}

pub fn edit(
    runtime: &ToolRuntime,
    add: &[String],
    remove: Option<&str>,
    value_stdin: bool,
    json_out: bool,
) -> u8 {
    let result = (|| {
        let key = remove
            .or_else(|| add.first().map(String::as_str))
            .ok_or("Supply --add KEY VALUE or --remove KEY.")?;
        let value = if remove.is_some() {
            None
        } else if value_stdin {
            if add.len() != 1 {
                return Err("Use --add KEY --value-stdin without a VALUE argument.");
            }
            let mut value = String::new();
            io::stdin()
                .take(65_537)
                .read_to_string(&mut value)
                .map_err(|_| "Cannot read value from stdin.")?;
            if value.len() > 65_536 {
                return Err("Value exceeds 64 KiB.");
            }
            if value.ends_with('\n') {
                value.pop();
                if value.ends_with('\r') {
                    value.pop();
                }
            }
            Some(value)
        } else {
            Some(
                add.get(1)
                    .ok_or("Supply --add KEY VALUE or --add KEY --value-stdin.")?
                    .clone(),
            )
        };
        Ok((key, value))
    })();
    let (key, value) = match result {
        Ok(value) => value,
        Err(message) => {
            emit_error(message, json_out);
            return 2;
        }
    };
    let home = &runtime.config().home;
    match octocode_native::config::edit_global_env(home, key, value.as_deref()) {
        Ok(changed) => {
            let path = home.join(".env");
            let action = if remove.is_some() { "remove" } else { "add" };
            if json_out {
                write_json(
                    &json!({"success": true, "action": action, "key": key, "path": path, "changed": changed}),
                    true,
                )
            } else {
                println!(
                    "{action} {key}: {} ({})",
                    path.display(),
                    if changed { "updated" } else { "unchanged" }
                );
                0
            }
        }
        Err(error) => {
            emit_error(&format!("Cannot edit global config: {error}"), json_out);
            if error.kind() == io::ErrorKind::InvalidInput {
                2
            } else {
                5
            }
        }
    }
}
