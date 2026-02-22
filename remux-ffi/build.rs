#[path = "../build/shared_git_metadata.rs"]
mod shared_git_metadata;

fn main() {
    shared_git_metadata::emit_git_metadata();
    generate_licenses_json();
}

fn generate_licenses_json() {
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::process::Command;

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let licenses_path = std::path::Path::new(&out_dir).join("licenses.json");

    // Don't re-run on every build — only when the lock file changes
    println!("cargo:rerun-if-changed=../Cargo.lock");

    let result = (|| -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["metadata", "--format-version", "1", "--frozen"])
            .output()?;

        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;

        // Collect workspace member package IDs
        let workspace_members: std::collections::HashSet<String> = metadata["workspace_members"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Build a lookup from package ID to package info
        let packages: BTreeMap<String, &serde_json::Value> = metadata["packages"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p["id"].as_str().map(|id| (id.to_string(), p)))
                    .collect()
            })
            .unwrap_or_default();

        // Walk resolve nodes for workspace members, collecting their direct deps
        let mut dep_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        if let Some(resolve) = metadata["resolve"]["nodes"].as_array() {
            for node in resolve {
                let node_id = node["id"].as_str().unwrap_or_default();
                if !workspace_members.contains(node_id) {
                    continue;
                }

                if let Some(deps) = node["deps"].as_array() {
                    for dep in deps {
                        // Only include normal (non-dev, non-build) dependencies
                        let is_normal = dep["dep_kinds"]
                            .as_array()
                            .map(|kinds| {
                                kinds
                                    .iter()
                                    .any(|k| k["kind"].is_null() || k["kind"].as_str() == Some(""))
                            })
                            .unwrap_or(false);

                        if is_normal {
                            if let Some(pkg_id) = dep["pkg"].as_str() {
                                dep_ids.insert(pkg_id.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Remove workspace members themselves from the dep set
        for member in &workspace_members {
            dep_ids.remove(member);
        }

        // Deduplicate by package name (keep first seen version if duplicates)
        let mut seen_names: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for dep_id in &dep_ids {
            if let Some(pkg) = packages.get(dep_id.as_str()) {
                let name = pkg["name"].as_str().unwrap_or_default().to_string();
                if seen_names.contains_key(&name) {
                    continue;
                }
                let version = pkg["version"].as_str().unwrap_or_default();
                let license = pkg["license"].as_str().unwrap_or_default();
                let authors = pkg["authors"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();

                let repository = pkg["repository"].as_str().unwrap_or_default();

                seen_names.insert(
                    name.clone(),
                    serde_json::json!({
                        "name": name,
                        "version": version,
                        "license": license,
                        "authors": authors,
                        "repository": repository,
                    }),
                );
            }
        }

        // BTreeMap is already sorted by name
        let entries: Vec<serde_json::Value> = seen_names.into_values().collect();
        Ok(serde_json::to_string_pretty(&entries)?)
    })();

    let json = result.unwrap_or_else(|_| "[]".to_string());
    let mut file = std::fs::File::create(&licenses_path).expect("Failed to create licenses.json");
    file.write_all(json.as_bytes())
        .expect("Failed to write licenses.json");
}
