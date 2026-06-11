defmodule AdoCli.ConfigFile do
  @moduledoc """
  Persistent configuration file at `~/.ado_cli/config.json`.

  Stores organization, auth method, and credentials for reuse across sessions.
  """

  @config_dir Path.join(System.user_home!(), ".ado_cli")
  @config_file Path.join(@config_dir, "config.json")

  @doc """
  Saves configuration to `~/.ado_cli/config.json`.

  Merges with existing configuration if present.
  """
  def save(new_config) do
    existing = load() || %{}
    merged = Map.merge(existing, new_config)
    File.mkdir_p!(@config_dir)
    File.write!(@config_file, JSON.encode!(merged))
    :ok
  end

  @doc """
  Loads the current configuration from `~/.ado_cli/config.json`.

  Returns a map or `nil` if no config exists.
  """
  def load do
    case File.read(@config_file) do
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
    File.rm_rf(@config_file)
  end

  @doc """
  Returns `true` if a configuration file exists.
  """
  def configured?, do: load() != nil
end
