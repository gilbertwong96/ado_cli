defmodule AdoCli.ConfigFile do
  @moduledoc """
  Persistent configuration file at `~/.ado_cli/config.json` by default.

  Stores organization, auth method, and credentials for reuse across sessions.

  The config path can be overridden via Application config for testing:
      Application.put_env(:ado_cli, :config_path, "/tmp/ado_cli_test.json")
  """

  @doc """
  Returns the resolved config file path (overridable for tests).
  Computed at runtime so the binary is portable across machines.
  """
  def config_path do
    Application.get_env(:ado_cli, :config_path) || default_config_path()
  end

  defp default_config_path do
    Path.join(System.user_home!(), ".ado_cli/config.json")
  end

  @doc """
  Returns the resolved config directory path.
  """
  def config_dir do
    Path.dirname(config_path())
  end

  @credential_keys ["pat", "token"]

  @doc """
  Saves configuration to the config file.

  Merges with existing config, preserving non-credential fields like
  `server` and `org`. When the `method` changes, stale credential
  fields (`pat`, `token`) from the previous method are cleared so
  they don't shadow the new auth method in `Auth.resolve_auth/0`.
  """
  def save(new_config) do
    existing = load() || %{}

    base =
      if method_changed?(existing, new_config),
        do: Map.drop(existing, @credential_keys),
        else: existing

    merged = Map.merge(base, new_config)

    File.mkdir_p!(config_dir())
    File.write!(config_path(), JSON.encode!(merged))
    :ok
  end

  defp method_changed?(%{"method" => old}, %{method: new}) when old != new, do: true
  defp method_changed?(%{"method" => old}, %{"method" => new}) when old != new, do: true
  defp method_changed?(_existing, _new), do: false

  @doc """
  Loads the current configuration. Returns a map or `nil` if no config exists.
  """
  def load do
    case File.read(config_path()) do
      {:ok, content} ->
        case JSON.decode(content) do
          {:ok, config} when is_map(config) -> config
          _ -> nil
        end

      {:error, _} ->
        nil
    end
  end

  @doc """
  Deletes the configuration file (logout).
  """
  def delete do
    File.rm_rf(config_path())
  end

  @doc """
  Returns `true` if a configuration file exists.
  """
  def configured?, do: load() != nil
end
