---
description: Azure DevOps CI/CD configuration — YAML pipelines, triggers, variables
version: "1.0"
---

# Azure DevOps CI/CD

This skill covers Azure DevOps CI/CD configuration for AI agents
attached to `ado_cli`.

## Pipeline Triggers

```yaml
trigger:
  branches:
    include:
      - main
      - releases/*
  paths:
    exclude:
      - docs/*

pr:
  branches:
    include:
      - main
```

## Pipeline Variables

```yaml
variables:
  buildConfiguration: Release
  - name: MAJOR_VERSION
    value: 1
  - group: my-variable-group
```

## Common Pipeline Tasks

### Build .NET
```yaml
- task: DotNetCoreCLI@2
  inputs:
    command: build
    projects: "**/*.csproj"
```

### Run Tests
```yaml
- task: DotNetCoreCLI@2
  inputs:
    command: test
    projects: "**/*Tests.csproj"
    arguments: "--configuration $(buildConfiguration) --collect:\"XPlat Code Coverage\""
```

### Publish Artifacts
```yaml
- task: PublishBuildArtifacts@1
  inputs:
    PathtoPublish: $(Build.ArtifactStagingDirectory)
    ArtifactName: drop
```

## Running Pipelines via CLI

```bash
# List pipelines
ado_cli pipelines list MyProject

# Trigger a run
ado_cli pipelines run MyProject 1 --branch feature/login

# With variables
ado_cli pipelines run MyProject 1 --variables "ENV=staging,DEBUG=true"
```

## Releases

```bash
# List releases
ado_cli releases list MyProject

# Show release details
ado_cli releases show MyProject 42
```
