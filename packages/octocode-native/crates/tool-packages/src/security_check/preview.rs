//! Describe identifiers that execution may send. The planner has no transport,
//! credential or cache; follow-up requests inferred from remote facts stay conditional.
use super::*;
use crate::artifact::preview::{destination, lookalike_destination, registries};

pub fn preview(
    query: &SecurityCheckQuery,
    github: &str,
    osv: &str,
) -> Result<Value, ArtifactError> {
    let mut destinations = Vec::new();
    let mut plan = Plan {
        github,
        osv,
        destinations: &mut destinations,
    };
    match query {
        SecurityCheckQuery::Package(r) => plan.package(
            r.ecosystem,
            &r.package_name,
            r.version.as_deref().map(String::as_str),
            r.release.as_deref().map(String::as_str),
        )?,
        SecurityCheckQuery::Repository(r) => plan.repository(&r.owner, &r.repo),
        SecurityCheckQuery::Commit(r) => plan.commit(&r.ref_, named(&r.owner, &r.repo)),
        SecurityCheckQuery::Advisory(r) => plan.advisory(&r.id, named(&r.owner, &r.repo)),
        SecurityCheckQuery::Url(r) => match url_source::github(&r.url)
            .transpose()
            .map_err(|e| ArtifactError::new(ErrorCode::InvalidInput, e.reason))?
        {
            Some(url_source::GithubUrl::Repository { owner, repo }) => {
                plan.repository(&owner, &repo);
            }
            Some(url_source::GithubUrl::Commit { owner, repo, sha }) => {
                plan.commit(&sha, Some((&owner, &repo)));
            }
            Some(url_source::GithubUrl::Advisory { id }) => plan.advisory(&id, None),
            None => {
                let coord = url_source::parse(&r.url)
                    .map_err(|e| ArtifactError::new(ErrorCode::InvalidInput, e.reason))?;
                plan.package(
                    coord.ecosystem,
                    &coord.name,
                    coord.version.as_deref(),
                    coord.release.as_deref(),
                )?;
            }
        },
    }
    Ok(json!({"preview":{"destinations":destinations}}))
}

struct Plan<'a> {
    github: &'a str,
    osv: &'a str,
    destinations: &'a mut Vec<Value>,
}

impl Plan<'_> {
    fn package(
        &mut self,
        ecosystem: ecosystem::Ecosystem,
        name: &str,
        version: Option<&str>,
        release: Option<&str>,
    ) -> Result<(), ArtifactError> {
        ecosystem::check_release(ecosystem, release)?;
        let mut coordinates = vec![
            format!("ecosystem={ecosystem}"),
            format!("packageName={name}"),
        ];
        if let Some(v) = version {
            coordinates.push(format!("version={v}"));
        }
        if let Some(v) = release {
            coordinates.push(format!("release={v}"));
        }
        let registry = ecosystem::registry(ecosystem)?;
        if let Some(kind) = registry {
            self.destinations
                .extend(registries(kind, None, true, &coordinates));
            if let Some(count) = lookalike_destination(kind, name, None) {
                self.destinations.push(count);
            }
            self.destinations.push(destination(
                "github",
                Some(self.github),
                &["repository, directory and release identifiers supplied by the registry".into()],
                Some("registry metadata identifies a GitHub source"),
            ));
        }
        let mut osv_sends = vec![
            format!("ecosystem={}", ecosystem::osv_name(ecosystem, release)),
            format!("packageName={name}"),
        ];
        if ecosystem::osv_orders_versions(ecosystem)? {
            if let Some(version) = version {
                osv_sends.push(format!("version={version}"));
            } else if registry.is_some() {
                osv_sends.push("version selected from registry metadata".into());
            }
            osv_sends.push("candidate fix versions when checking an upgrade".into());
        }
        osv_sends
            .push("advisory IDs and aliases from returned records when cross-checking them".into());
        self.destinations
            .push(destination("osv", Some(self.osv), &osv_sends, None));
        if release.is_none()
            && let Some(gh_ecosystem) =
                octocode_runtime_base::contracts::security::security_contract()
                    .ok()
                    .and_then(|contract| contract.github_ecosystem_of(&ecosystem.to_string()))
        {
            let mut github_coordinates = vec![
                format!("ecosystem={gh_ecosystem}"),
                format!("packageName={name}"),
            ];
            if let Some(version) = version {
                github_coordinates.push(format!("version={version}"));
            } else {
                github_coordinates.push("confirmed release version".into());
            }
            self.destinations.push(destination(
                "github",
                Some(self.github),
                &github_coordinates,
                Some("checking GitHub's advisories for the confirmed release"),
            ));
        }
        Ok(())
    }
    fn repository(&mut self, owner: &str, repo: &str) {
        self.destinations.push(destination(
            "github",
            Some(self.github),
            &[format!("repository={owner}/{repo}")],
            None,
        ));
        self.destinations.push(destination(
            "osv",
            Some(self.osv),
            &["public advisory IDs reported by GitHub".into()],
            Some("GitHub confirms the repository is public and lists advisories"),
        ));
    }
    fn commit(&mut self, sha: &str, repo: Option<(&str, &str)>) {
        self.destinations.push(destination(
            "osv",
            Some(self.osv),
            &[format!("commit={sha}")],
            None,
        ));
        if let Some((owner, repo)) = repo {
            self.destinations.push(destination(
                "github",
                Some(self.github),
                &[
                    format!("repository={owner}/{repo}"),
                    format!("commit={sha}"),
                ],
                None,
            ));
        }
    }
    fn advisory(&mut self, id: &str, repo: Option<(&str, &str)>) {
        self.destinations.push(destination(
            "osv",
            Some(self.osv),
            &[
                format!("advisory={id}"),
                "aliases from returned records".into(),
            ],
            None,
        ));
        if id.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("GHSA-")) {
            let mut sends = vec![format!("advisory={}", canonical_advisory_id(id))];
            if let Some((owner, repo)) = repo {
                sends.push(format!("repository={owner}/{repo}"));
            }
            self.destinations
                .push(destination("github", Some(self.github), &sends, None));
        }
    }
}
