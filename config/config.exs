import Config

# Azure DevOps configuration
# Set via environment variables:
#   ADO_ORG        - Azure DevOps organization name
#   ADO_PAT        - Personal Access Token
#   ADO_API_VERSION - API version (default: "7.1")

config :ado_cli, :azure_devops,
  org: System.get_env("ADO_ORG"),
  pat: System.get_env("ADO_PAT"),
  server: System.get_env("ADO_SERVER"),
  api_version: System.get_env("ADO_API_VERSION", "7.1")

# Finch configuration
config :ado_cli, AdoCli.Client,
  base_url: "https://dev.azure.com",
  vsrm_base_url: "https://vsrm.dev.azure.com",
  accept:
    "application/json; api-version=7.1-preview.3; excludeUrls=true; enumsAsNumbers=true; msDateFormat=true; noArrayWrap=true"

# Finch pool configuration
config :finch,
  pools: %{
    default: [size: 5, count: 1],
    ado: [size: 5, count: 1]
  }
