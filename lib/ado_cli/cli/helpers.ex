defmodule AdoCli.CLI.Helpers do
  @moduledoc """
  Shared helpers for CLI command modules.

  Provides common patterns for API response handling, JSON/table output
  switching, and error formatting to keep command functions lean.
  """

  import CliMate.CLI

  @doc """
  Handles a `{:ok, data} | {:error, reason}` result from the API client.

  On success, calls `success_fn` with the data (which should format + halt).
  On error, formats and halts with an appropriate message.
  """
  @spec handle_api_result(
          {:ok, term()} | {:error, term()},
          map(),
          (term() -> no_return())
        ) :: no_return()
  def handle_api_result({:ok, data}, _parsed, success_fn) do
    success_fn.(data)
  end

  def handle_api_result({:error, :not_configured}, _parsed, _success_fn) do
    halt_error(
      "Not authenticated. Run 'ado_cli login --method pat --org ORG --pat TOKEN' or set ADO_ORG+ADO_PAT."
    )
  end

  def handle_api_result({:error, %{status: status, body: body}}, _parsed, _success_fn) do
    halt_error("API error #{status}: #{inspect(body)}")
  end

  def handle_api_result({:error, reason}, _parsed, _success_fn) do
    halt_error("Request failed: #{inspect(reason)}")
  end

  @doc """
  Outputs the given data as pretty JSON if --json was set,
  otherwise calls the formatter function, then halts.
  """
  @spec json_or_format(data :: term(), parsed :: map(), formatter_fn :: (term() -> :ok)) ::
          no_return()
  def json_or_format(data, parsed, formatter_fn) do
    if parsed.options.json do
      # Use apply/3 to avoid gradualizer type tracing through JSON.encode_to_iodata!/2
      writeln(apply(JSON, :encode_to_iodata!, [data, [pretty: true]]))
    else
      formatter_fn.(data)
    end

    halt_success("")
  end
end
