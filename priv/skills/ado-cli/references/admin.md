# Administration: Security, Connections, Pools, Wikis, Iterations, Areas

## Service Connections


```bash
# List connections (table: ID, Name, Type)
ado connections list MyProject
ado connections list MyProject --type github

# Show details (ID, name, type, url, isReady). Secrets are never returned.
ado connections show MyProject <connection-id>

# Create a GitHub PAT connection (literal token — appears in shell history)
ado connections create MyProject GitHubPat github https://github.com \
    --access-token gh_xxxxx --description "CI bot PAT"

# Create with token from stdin (secure — no shell history)
echo "$GITHUB_PAT" | ado connections create MyProject GitHubPat github https://github.com \
    --access-token -

# Create with token from a file
ado connections create MyProject GitHubPat github https://github.com \
    --access-token @~/.github-pat --description "CI bot PAT"

# Create with type-specific --data (e.g. Azure RM subscription)
ado connections create MyProject AzureProd azure "" \
    --data '{"subscriptionId":"11111111-2222-3333-4444-555555555555","subscriptionName":"Prod"}' \
    --scheme UsernamePassword --access-token secret-password

# Update (rename, change description, or rotate credentials)
ado connections update MyProject <id> --name "Renamed"
ado connections update MyProject <id> --access-token new-token
ado connections update MyProject <id> --data '{"subscriptionId":"new-sub-id"}'
# No fields supplied → usage error

# Delete (--force skips y/N confirmation)
ado connections delete MyProject <id> --force
```

JSON response shape for create/update (with `--json`):
```json
{
  "ok": true,
  "result": {
    "id": "uuid",
    "name": "string",
    "type": "string",
    "url": "string",
    "isReady": true
  }
}
```


## Security


```bash
ado security groups list MyProject
ado security groups create MyProject --name "Reviewers"
ado security groups members list MyProject "vssgp.xxxxx"
ado security permissions list "2e9eb7ed-..." --token "repoV2/projectId/repoId"
```


## Agent Pools


```bash
ado agent-pools list
ado agent-pools show POOL_ID
ado agent-pools queues POOL_ID
```


## Banners (Org Notifications)


```bash
ado banners show
ado banners set --message "Maintenance window: Sat 2-4am" --type warning
ado banners delete
```


## Wikis


```bash
ado wikis list MyProject
ado wikis pages list MyProject MyWiki
ado wikis pages show MyProject MyWiki --path /Home
ado wikis pages create MyProject MyWiki --path /Design --content "# Design Doc"
```


## Iterations (Sprints)


```bash
ado iterations list MyProject
ado iterations show MyProject MyTeam "Sprint 23"
ado iterations create MyProject MyTeam --name "Sprint 24" --start-date 2026-01-15 --finish-date 2026-01-29
```


## Areas


```bash
ado areas list MyProject
ado areas show MyProject "MyArea"
ado areas create MyProject --name "NewArea"
```


## Skills (for AI agents)


```bash
ado skills list
ado skills describe ado-cli
ado skills read ado-cli
ado skills search "pipeline"
ado skills install                                   # install to all known agent dirs
ado skills install --target pi --skill ado-cli
```
