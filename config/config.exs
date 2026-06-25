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
  api_version: System.get_env("ADO_API_VERSION", "7.1"),
  oauth_client_id: System.get_env("ADO_OAUTH_CLIENT_ID", "c33cb54f-f0d0-45e4-9aa7-5a4ee42b2b2c")

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

# Logger configuration
#
# The CLI uses CliMate to write its own output to stdout. Library logs
# (Finch, Bandit, CliMate internals) must NOT pollute stdout because users
# pipe CLI output to `head`, `delta`, `code --diff`, etc. — a broken pipe
# would surface as "Failed to write log message to stdout, trying stderr".
#
# Two safeguards:
#   1. Route the console handler to :standard_error so library logs never
#      mix with the CLI's stdout output.
#   2. Raise the default level to :error so dependency INFO/WARN chatter
#      (Finch request logs, Bandit debug, etc.) is suppressed by default.
#      Users can still enable verbose logs via ELIXIR_ERL_OPTIONS or by
#      setting ADO_LOG_LEVEL=debug in the environment.
log_level =
  case System.get_env("ADO_LOG_LEVEL") do
    nil -> :error
    "debug" -> :debug
    "info" -> :info
    "warning" -> :warning
    "error" -> :error
    _ -> :error
  end

config :logger,
  level: log_level,
  handle_otp_reports: false,
  handle_sasl_reports: false

config :logger, :console,
  format: "$time $level [$metadata] $message\n",
  metadata: [],
  # Route library logs to stderr so they don't interleave with CLI stdout
  # and so `ado ... | head` doesn't trigger EPIPE on the log handler.
  device: :standard_error
