---
description: Azure DevOps REST API reference — endpoints, areas, operations
version: "7.1"
---

# Azure DevOps API Skill

This skill provides the Azure DevOps REST API reference for AI agents
attached to `ado_cli`. Use it to construct correct API calls.

## API Base URL

```
https://dev.azure.com/{organization}
```

> For MSA-backed orgs (`*.visualstudio.com`), use `https://{org}.visualstudio.com`.

## API Version

All requests require `api-version=7.1` query parameter.

## Core API (`_apis/projects`)

| Operation | Method | Path |
|-----------|--------|------|
| List projects | GET | `_apis/projects` |
| Get project | GET | `_apis/projects/{projectId}` |
| Create project | POST | `_apis/projects?api-version=7.1` |

Query parameters:
- `stateFilter`: `wellFormed`, `createPending`, `deleting`, `new`, `all`
- `$top`: max results
- `$skip`: skip N results

## Git API (`_apis/git/repositories`)

| Operation | Method | Path |
|-----------|--------|------|
| List repos | GET | `{project}/_apis/git/repositories` |
| Get repo | GET | `{project}/_apis/git/repositories/{repoId}` |
| Create repo | POST | `{project}/_apis/git/repositories?api-version=7.1` |
| List branches | GET | `{project}/_apis/git/repositories/{repoId}/refs?filter=heads/` |

## Work Items API (`_apis/wit/workitems`)

| Operation | Method | Path |
|-----------|--------|------|
| Get work item | GET | `_apis/wit/workitems/{id}` |
| Create work item | POST | `{project}/_apis/wit/workitems/${type}` |
| Update work item | PATCH | `_apis/wit/workitems/{id}` |
| WIQL query | POST | `{project}/_apis/wit/wiql` |

Create body (JSON Patch):
```json
[
  {"op": "add", "path": "/fields/System.Title", "value": "Title"}
]
```

## Build/Pipelines API (`_apis/pipelines`)

| Operation | Method | Path |
|-----------|--------|------|
| List pipelines | GET | `{project}/_apis/pipelines` |
| Get pipeline | GET | `{project}/_apis/pipelines/{pipelineId}` |
| Run pipeline | POST | `{project}/_apis/pipelines/{pipelineId}/runs` |

## Pull Requests API (`_apis/git/repositories/{repoId}/pullrequests`)

| Operation | Method | Path |
|-----------|--------|------|
| List PRs | GET | `{project}/_apis/git/repositories/{repoId}/pullrequests` |
| Get PR | GET | `{project}/_apis/git/repositories/{repoId}/pullrequests/{prId}` |
| Create PR | POST | `{project}/_apis/git/repositories/{repoId}/pullrequests` |
| Complete PR | PATCH | `{project}/_apis/git/repositories/{repoId}/pullrequests/{prId}` |

## Release API (`_apis/release/releases`)

| Operation | Method | Path |
|-----------|--------|------|
| List releases | GET | `{project}/_apis/release/releases` |
| Get release | GET | `{project}/_apis/release/releases/{releaseId}` |
