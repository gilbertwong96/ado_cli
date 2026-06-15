defmodule AdoCli.Version do
  @moduledoc """
  Resolves the running ado version across different runtime contexts.

  Works in:
    * dev (`mix run ...`) — `Mix.Project.config()[:version]`
    * test (`mix test`)   — same as dev
    * escript (`mix escript.build`) — `Application.spec(:ado_cli, :vsn)`
    * Burrito binary (`MIX_ENV=prod mix release`) — same as escript

  Always returns a `String.t/0`. Falls back to `"unknown"` if neither
  Mix.Project nor Application.spec can provide a version.
  """

  @doc "Return the current version of ado, as a string."
  @spec current() :: String.t()
  def current do
    # In escript/Burrito builds, Mix.Project.config() raises
    # FunctionClauseError because Mix isn't actually running. The try
    # ensures we fall through to Application.spec/1 in that case.
    case safe_mix_project_version() do
      {:ok, v} -> v
      :error -> ado_cli_app_vsn()
    end
  end

  # Try to read the version from Mix.Project. Returns {:ok, "0.1.0"} on
  # success, :error otherwise (e.g. when running as an escript).
  defp safe_mix_project_version do
    if Code.ensure_loaded?(Mix.Project) and function_exported?(Mix.Project, :config, 0) do
      try do
        case Mix.Project.config() do
          conf when is_list(conf) ->
            case Keyword.get(conf, :version) do
              nil -> :error
              v -> {:ok, to_string(v)}
            end

          _ ->
            :error
        end
      rescue
        _ -> :error
      catch
        _, _ -> :error
      end
    else
      :error
    end
  end

  # Get the version from the loaded application's vsn field. In escript
  # mode the .app file is bundled but Application.load/1 hasn't been
  # called yet, so Application.spec/2 returns nil. We load it on
  # demand here.
  defp ado_cli_app_vsn do
    Application.ensure_loaded(:ado_cli)

    case Application.spec(:ado_cli, :vsn) do
      nil -> "unknown"
      :undefined -> "unknown"
      "" -> "unknown"
      vsn when is_binary(vsn) -> vsn
      vsn when is_list(vsn) -> List.to_string(vsn)
      vsn -> to_string(vsn)
    end
  end
end
