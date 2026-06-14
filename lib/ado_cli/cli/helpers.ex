defmodule AdoCli.CLI.Helpers do
  @moduledoc """
  Shared helpers for CLI command modules.

  Provides common patterns for API response handling, JSON/table output
  switching, and error formatting to keep command functions lean.

  All helpers in this module honor the global `--json` flag. When set,
  output is a structured JSON envelope (see `AdoCli.CLI.Output` for the
  shape); otherwise it's human-readable formatted text.
  """

  alias AdoCli.CLI.Output

  @doc """
  Handles a `{:ok, data} | {:error, reason}` result from the API client.

  On success, calls `success_fn` with the data. The success_fn is
  expected to format + halt (use `Helpers.json_or_format/3` or
  `AdoCli.CLI.Output.ok/4`).

  On error, prints a structured error envelope (when `--json` is set)
  or a colored human message (otherwise) and halts with a non-zero
  exit code.

  Recognized error shapes:
    * `{:error, :not_configured}`    → code "auth_required", exit 3
    * `{:error, %{status: s, body: b}}` → code "api_error", exit 4
    * `{:error, reason}`             → code "network_error", exit 5
  """
  @spec handle_api_result(
          {:ok, term()} | {:error, term()},
          map(),
          (term() -> no_return())
        ) :: no_return()
  def handle_api_result({:ok, data}, _parsed, success_fn) do
    success_fn.(data)
  end

  def handle_api_result({:error, :not_configured}, parsed, _success_fn) do
    Output.error(
      parsed,
      "auth_required",
      "Not authenticated. Run 'ado_cli login --method pat --org ORG --pat TOKEN' or set ADO_ORG+ADO_PAT.",
      details: %{
        "hint" =>
          "Set ADO_ORG and ADO_PAT env vars, or run `ado login --method pat --org ORG --pat TOKEN`",
        "scopes" => "PAT must have: vso.work, vso.code, vso.project, vso.build, vso.release"
      }
    )
  end

  def handle_api_result({:error, %{status: status, body: body}}, parsed, _success_fn) do
    {code, message} = classify_api_error(status, body)

    Output.error(
      parsed,
      code,
      message,
      status: status,
      details: %{
        "status" => status,
        "body" => if(is_binary(body), do: body, else: inspect(body, limit: 50))
      }
    )
  end

  def handle_api_result({:error, reason}, parsed, _success_fn) do
    {code, message} = classify_network_error(reason)

    Output.error(
      parsed,
      code,
      message,
      details: %{"reason" => inspect(reason, limit: 50)}
    )
  end

  @doc """
  Outputs the given data as pretty JSON if `--json` was set,
  otherwise calls the formatter function, then halts with success.

  This is a thin wrapper around `AdoCli.CLI.Output.ok/4` for
  backward compat with existing callers.
  """
  @spec json_or_format(data :: term(), parsed :: map(), formatter_fn :: (term() -> :ok)) ::
          no_return()
  def json_or_format(data, parsed, formatter_fn) do
    Output.ok(parsed, data, :value, fn value ->
      formatter_fn.(value)
    end)
  end

  @doc """
  Like `json_or_format/3` but for a list of items. Adds a `count`
  field to the JSON envelope.
  """
  @spec json_or_format_list([term()], map(), (term() -> :ok)) :: no_return()
  def json_or_format_list(items, parsed, formatter_fn) do
    Output.ok(parsed, items, :list, fn list ->
      formatter_fn.(list)
    end)
  end

  # ── error classification ──────────────────────────────────────────────

  @doc """
  Classify an HTTP error into a `(code, message)` pair.

  Exposed for testing.
  """
  @spec classify_api_error(integer(), term()) :: {String.t(), String.t()}
  def classify_api_error(status, body) do
    body_str = if is_binary(body), do: body, else: inspect(body, limit: 200)
    api_error_classification(status, body_str)
  end

  defp api_error_classification(302, _),
    do: {"auth_required", "API redirected to sign-in page. Run 'ado login' to authenticate."}

  defp api_error_classification(401, _),
    do: {"auth_required", "Authentication failed. PAT is invalid or expired."}

  defp api_error_classification(403, _),
    do:
      {"forbidden",
       "Forbidden — your PAT lacks the required scope, or you don't have access to this resource."}

  defp api_error_classification(404, _),
    do: {"not_found", "Resource not found. Check the project/repo/build ID and your permissions."}

  defp api_error_classification(409, _),
    do: {"conflict", "Conflict — the resource already exists or is in an invalid state."}

  defp api_error_classification(422, _),
    do: {"validation_error", "Azure DevOps rejected the request as invalid."}

  defp api_error_classification(429, _),
    do: {"forbidden", "Rate limited by Azure DevOps. Slow down and retry."}

  defp api_error_classification(s, _) when s >= 500 and s < 600,
    do: {"api_error", "Azure DevOps server error. Retry later."}

  defp api_error_classification(s, body_str), do: {"api_error", "API error #{s}: #{body_str}"}

  @doc """
  Classify a non-HTTP error (timeout, DNS, connection refused) into a
  `(code, message)` pair.

  Exposed for testing.
  """
  @spec classify_network_error(term()) :: {String.t(), String.t()}
  def classify_network_error(reason), do: network_error_classification(reason)

  defp network_error_classification(:timeout),
    do: {"network_error", "Request timed out. Check your network connection."}

  defp network_error_classification(:nxdomain),
    do: {"network_error", "DNS lookup failed. Check the server URL."}

  defp network_error_classification(:econnrefused),
    do: {"network_error", "Connection refused. Is the server reachable?"}

  defp network_error_classification(reason) when is_atom(reason),
    do: {"network_error", "Request failed: #{inspect(reason)}"}

  defp network_error_classification(reason),
    do: {"network_error", "Request failed: #{inspect(reason, limit: 200)}"}
end
