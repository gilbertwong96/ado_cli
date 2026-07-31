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


### Secure Files

```bash
# List secure files in a project
ado pipelines secure_files list MyProject

# Show details of one secure file
ado pipelines secure_files show MyProject <id>

# Upload a local file (cert, kubeconfig, signing key)
ado pipelines secure_files upload MyProject prod-cert.pem --file ./prod-cert.pem

# Replace an existing file with the same name
ado pipelines secure_files upload MyProject prod-cert.pem --file ./new.pem --allow-exists

# Delete (requires --force to confirm)
ado pipelines secure_files delete MyProject <id> --force
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

