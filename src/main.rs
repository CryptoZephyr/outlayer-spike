use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use wasi_http_client::Client;

#[derive(Deserialize)]
struct Input {
    repo: String,
    tag: String,
}

#[derive(Deserialize)]
struct Release {
    published_at: Option<String>,
    target_commitish: Option<String>,
}

#[derive(Serialize)]
struct Output {
    ok: bool,
    repo: String,
    tag: String,
    published_at: Option<String>,
    target_commitish: Option<String>,
    error: Option<String>,
}

fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let Some(owner) = parts.next() else {
        return false;
    };
    let Some(name) = parts.next() else {
        return false;
    };

    parts.next().is_none()
        && !owner.is_empty()
        && !name.is_empty()
        && repo.len() <= 120
        && [owner, name].iter().all(|part| {
            part.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 100
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/+".contains(&byte))
}

fn result(repo: &str, tag: &str, error: &str) -> Output {
    Output {
        ok: false,
        repo: repo.to_string(),
        tag: tag.to_string(),
        published_at: None,
        target_commitish: None,
        error: Some(error.to_string()),
    }
}

fn run(input: Input) -> Output {
    if !valid_repo(&input.repo) || !valid_tag(&input.tag) {
        return result(&input.repo, &input.tag, "invalid_input");
    }

    let encoded_tag: String = input
        .tag
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect();
    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        input.repo, encoded_tag
    );
    let response = match Client::new()
        .get(&url)
        .header("User-Agent", "near-spike-release-checker")
        .header("Accept", "application/vnd.github+json")
        .send()
    {
        Ok(response) => response,
        Err(_) => return result(&input.repo, &input.tag, "request_failed"),
    };

    if response.status() == 404 {
        return result(&input.repo, &input.tag, "release_not_found");
    }
    if response.status() != 200 {
        return result(
            &input.repo,
            &input.tag,
            &format!("github_http_{}", response.status()),
        );
    }

    let body = match response.body() {
        Ok(body) => body,
        Err(_) => return result(&input.repo, &input.tag, "response_read_failed"),
    };
    let release: Release = match serde_json::from_slice(&body) {
        Ok(release) => release,
        Err(_) => return result(&input.repo, &input.tag, "invalid_github_response"),
    };

    Output {
        ok: true,
        repo: input.repo,
        tag: input.tag,
        published_at: release.published_at,
        target_commitish: release.target_commitish,
        error: None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let output = match serde_json::from_str::<Input>(&input) {
        Ok(input) => run(input),
        Err(_) => result("", "", "invalid_input"),
    };
    let json = serde_json::to_string(&output)?;
    println!("{json}");
    io::stdout().flush()?;
    Ok(())
}
