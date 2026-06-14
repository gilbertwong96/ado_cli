defmodule AdoCli.CLI.Output do
  @moduledoc """
  Structured output helpers for LLM-friendly JSON envelopes.

  This module gives every `ado` command a consistent shape for both
  its success and error output, so an LLM agent can reliably parse the
  result without resorting to regex on English text.

  ## Output shape (when `--json` is set)

      # Success: a single value
      {
        "ok": true,
        "result": { ...the actual data... }
      }

      # Success: a list
      {
        "ok": true,
        "count": 3,
        "items": [ ... ]
      }

      # Success: a single item by ID
      {
        "ok": true,
        "result": { ...the item... }
      }

      # Error: structured
      {
        "ok": false,
        "error": {
          "code": "auth_required",      # stable machine-readable code
          "status": 302,                # optional: HTTP status
          "message": "Not authenticated. Run 'ado login' ...",
          "details": { ... }            # optional: extra context
        }
      }

  ## Error codes

  These are the stable, machine-readable codes used in the
  `error.code` field. LLMs can match on these directly without
  parsing English:

    * `"auth_required"`        — not logged in (no PAT, no env vars, no config)
    * `"not_found"`            — resource (project, repo, build, etc.) not found
    * `"validation_error"`     — invalid arguments / missing required field
    * `"api_error"`            — Azure DevOps returned 4xx/5xx
    * `"network_error"`        — connection failed, timeout, DNS, etc.
    * `"forbidden"`            — PAT lacks required scope
    * `"conflict"`             — resource already exists or in invalid state
    * `"cancelled"`            — operation canceled by user
    * `"unknown"`              — catch-all for unclassified errors

  Add new codes here (and to the `format_error_code/1` switch below)
  when introducing a new error class — don't sprinkle new strings
  across the codebase.
  """

  import CliMate.CLI

  @doc """
  Print a successful result. Honors `--json`.

  `kind` is one of:
    * `:value` — a single value, wrapped as `{"ok": true, "result": data}`
    * `:list`  — a list of items, wrapped as `{"ok": true, "count": N, "items": data}`
    * `:raw`   — a single value, but emit it without the envelope (no `ok` wrapper)

  `formatter` is called for the human-readable path. Defaults to printing
  `value` via `IO.inspect(value, pretty: true)`.
  """
  @spec ok(map(), term(), atom(), (term() -> any())) :: no_return()
  def ok(parsed, value, kind \\ :value, formatter \\ &default_formatter/1) do
    if json?(parsed) do
      payload =
        case kind do
          :value -> %{ok: true, result: value}
          :list -> %{ok: true, count: length(List.wrap(value)), items: List.wrap(value)}
          :raw -> value
          _ -> %{ok: true, result: value}
        end

      # Use IO.puts (not writeln) to avoid ANSI color codes that would
      # pollute the JSON envelope. Then halt(0) directly — NOT
      # halt_success('') which would print a green "Done." marker.
      IO.puts(JSON.encode!(payload))
    else
      formatter.(value)
    end

    halt(0)
  end

  @doc """
  Print an error result. Honors `--json`.

  `code` is one of the stable error codes listed in the moduledoc.
  `message` is a human-readable explanation.
  `details` (optional) is a map of extra context (e.g. the HTTP body
  of an upstream error).

  Always exits with status 1 (matching the existing convention).
  LLMs should match on `error.code` in the JSON envelope, not on the
  exit code, which is reserved for shell scripting.
  """
  @spec error(map(), String.t(), String.t(), keyword()) :: no_return()
  def error(parsed, code, message, opts \\ []) do
    status = opts[:status]
    details = opts[:details]

    if json?(parsed) do
      err = %{"code" => code, "message" => message}
      err = if status, do: Map.put(err, "status", status), else: err
      err = if details, do: Map.put(err, "details", details), else: err
      # Use IO.puts (not writeln) to avoid ANSI color codes from
      # halt_success/1 polluting the JSON envelope. Then halt(1)
      # directly — NOT halt_success/1 which would print a green
      # "Done." marker.
      IO.puts(JSON.encode!(%{ok: false, error: err}))
    else
      colorize_error(code, message, status)
    end

    halt(1)
  end

  @doc """
  Print a success message (no data payload). Honors `--json`.
  """
  @spec ok_message(map(), String.t()) :: no_return()
  def ok_message(parsed, message) do
    if json?(parsed) do
      IO.puts(JSON.encode!(%{ok: true, message: message}))
    else
      writeln(message)
    end

    halt(0)
  end

  @doc """
  Emit a raw value (no envelope) when `--json` is set. For
  backward compat with commands that historically printed raw JSON.
  Use sparingly; prefer `ok/4` for new code.
  """
  @spec raw(map(), term()) :: no_return()
  def raw(parsed, value) do
    if json?(parsed), do: IO.puts(JSON.encode!(value))
    halt(0)
  end

  # ── error code → exit code mapping ───────────────────────────────────

  @doc """
  Returns the process exit code for a given error code.
  """
  @spec exit_code_for(String.t()) :: non_neg_integer()
  def exit_code_for("validation_error"), do: 2
  def exit_code_for("auth_required"), do: 3
  def exit_code_for("forbidden"), do: 3
  def exit_code_for("api_error"), do: 4
  def exit_code_for("network_error"), do: 5
  def exit_code_for(_), do: 1

  # ── helpers ──────────────────────────────────────────────────────────

  defp json?(%{options: %{json: true}}), do: true
  defp json?(%{options: %{json: _}}), do: false
  defp json?(_), do: false

  defp default_formatter(value) do
    IO.puts(inspect(value, pretty: true, limit: :infinity, printable_limit: :infinity))
  end

  defp colorize_error(code, message, status) do
    code_label = error_code_label(code)
    status_part = if status, do: " (#{status})", else: ""
    IO.puts(:io_lib.format("~s[~s~s] ~s~n", [red(), code_label, status_part, message]))
  end

  defp error_code_label("auth_required"), do: "Auth required"
  defp error_code_label("not_found"), do: "Not found"
  defp error_code_label("validation_error"), do: "Validation error"
  defp error_code_label("forbidden"), do: "Forbidden"
  defp error_code_label("api_error"), do: "API error"
  defp error_code_label("network_error"), do: "Network error"
  defp error_code_label("conflict"), do: "Conflict"
  defp error_code_label("cancelled"), do: "Cancelled"
  defp error_code_label(_), do: "Error"

  defp red, do: IO.ANSI.red() <> IO.ANSI.bright()
end
