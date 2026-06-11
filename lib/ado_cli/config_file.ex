defmodule AdoCli.ConfigFile do
  @moduledoc """
  Persistent configuration file at `~/.ado_cli/config.json` by default.

  Stores organization, auth method, and credentials for reuse across sessions.

  The config path can be overridden via Application config for testing:
      Application.put_env(:ado_cli, :config_path, "/tmp/ado_cli_test.json")
  """

  @default_config_dir Path.join(System.user_home!(), ".ado_cli")
  @default_config_file Path.join(@default_config_dir, "config.json")

  @doc """
  Returns the resolved config file path (overridable for tests).
  """
  def config_path do
    Application.get_env(:ado_cli, :config_path) || @default_config_file
  end

  @doc """
  Returns the resolved config directory path.
  """
  def config_dir do
    Path.dirname(config_path())
  end

  @doc """
  Saves configuration to the config file. Merges with existing configuration.
  """
  def save(new_config) do
    existing = load() || %{}
    merged = Map.merge(existing, new_config)
    File.mkdir_p!(config_dir())
    File.write!(config_path(), JSON.encode!(merged))
    :ok
  end

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
