# Pipelines, Builds & CI Watch

## Pipelines (YAML)


```bash
# List pipelines in a project
ado pipelines list MyProject

# Trigger a run
ado pipelines run MyProject 42 --branch main
ado pipelines run MyProject 42 --branch release --variables "ENV=staging,DEBUG=true"

# Variable groups
ado pipelines vars list MyProject
ado pipelines vars show MyProject 5
ado pipelines vars create MyProject --name "prod-secrets" --variables "DB_HOST=prod" --secret NPM_TOKEN
```


### Classic Builds

```bash
ado pipelines-builds queue MyProject --definition 5 --branch main
ado pipelines-builds cancel MyProject 99
ado pipelines-builds show MyProject 99
```


### CI Watch (Live Pipeline Logs)

```bash
ado ci watch MyProject 99                                         # specific build
ado ci watch MyProject --latest --definition 42 --branch main    # latest matching build
```

