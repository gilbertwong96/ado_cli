defmodule AdoCli.TestServer do
  @moduledoc """
  Minimal HTTP test server backed by Bandit.

  Drop-in replacement for the Bypass-based test setup. Each
  `expect/3` registers a request matcher; the next matching request
  invokes the handler and returns its response. Unmatched requests
  return 500 with a clear error message.

  ## Why Bandit, not Bypass

  Bypass 2.1 (the version on hex.pm) is hard-pinned to Cowboy and
  hasn't been updated since 2020. This project uses Bandit as the
  preferred HTTP server throughout. Writing a small Bandit-based test
  server is straightforward and keeps the dependency tree Cowboy-free.

  ## Usage

      server = AdoCli.TestServer.start_link()
      AdoCli.TestServer.expect(server, "GET", "/foo", fn conn ->
        AdoCli.TestServer.resp(conn, 200, "hello")
      end)

      url = AdoCli.TestServer.url(server) <> "/foo"
      HTTPoison.get!(url)
  """

  use GenServer

  defstruct port: nil, bandit_pid: nil, expectations: []

  @type method :: String.t()
  @type path :: String.t()
  @type handler :: (Plug.Conn.t() -> Plug.Conn.t())

  ## Public API

  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: name(opts))
  end

  defp name(_opts), do: :"#{__MODULE__}"

  @doc "URL base for the test server, e.g. \"http://127.0.0.1:12345\""
  @spec url(pid()) :: String.t()
  def url(server) do
    %{port: port} = :sys.get_state(server)
    "http://127.0.0.1:#{port}"
  end

  @doc "Register a request expectation. Single-shot — first match consumes it."
  @spec expect(pid(), method(), path(), handler()) :: :ok
  def expect(server, method, path, handler) do
    GenServer.call(server, {:expect, method, path, handler})
  end

  @doc "Number of pending expectations."
  @spec expectation_count(pid()) :: non_neg_integer()
  def expectation_count(server) do
    GenServer.call(server, :count)
  end

  @doc "Stop the test server."
  @spec stop(pid()) :: :ok
  def stop(server) do
    if Process.alive?(server), do: GenServer.stop(server)
    :ok
  end

  ## Plug — invoked for every request

  # NOTE: This module is intentionally NOT nested inside TestServer. The
  # nested-module approach (e.g. `defmodule AdoCli.TestServer.Plug do`)
  # would require fully-qualified `Plug.Conn` references everywhere
  # because `Plug.Conn` would resolve to `AdoCli.TestServer.Plug.Conn`.
  # Keeping it at top-level keeps the code clean.
  defmodule Plug do
    @moduledoc false
    # Nested-module gotcha: `Plug` inside `AdoCli.TestServer.Plug` refers
    # to the inner Plug (this module), so `@behaviour Plug` would refer
    # to itself and `@behaviour Plug` would be treated as a self-reference.
    # Use the full `Elixir.Plug` name to disambiguate.
    @behaviour Elixir.Plug

    # Same disambiguation for `Plug.Conn`.
    import Elixir.Plug.Conn, only: [put_resp_content_type: 2, resp: 3]

    @impl true
    def init(opts), do: opts

    @impl true
    def call(conn, opts) do
      # The TestServer GenServer registers itself in :persistent_term
      # during init. We look it up here. This works because Bandit
      # calls call/2 in the same process tree (a worker of the TestServer
      # supervisor), so the persistent_term is set by the time we get here.
      server = :persistent_term.get({__MODULE__.Server, :pid}, nil)

      if server do
        handle_request(server, conn)
      else
        conn
        |> put_resp_content_type("text/plain")
        |> resp(500, "TestServer not running")
      end
    end

    defp handle_request(server, conn) do
      case GenServer.call(server, {:pop, conn.method, conn.request_path}) do
        {:ok, handler} ->
          handler.(conn)

        :error ->
          conn
          |> put_resp_content_type("text/plain")
          |> resp(500, "TestServer: no expectation matched")
      end
    end
  end

  ## GenServer

  @impl true
  def init(_opts) do
    # Register ourselves so the Plug (running in the Bandit process tree)
    # can find us. We clean this up in terminate/2.
    :persistent_term.put({__MODULE__.Plug.Server, :pid}, self())

    # Start Bandit. Port 0 means "OS-assigned free port". We use
    # ThousandIsland's `listener_info/1` to read the bound port back
    # after Bandit is up.
    case start_bandit() do
      {:ok, bandit_pid, port} ->
        {:ok, %__MODULE__{bandit_pid: bandit_pid, port: port}}

      {:error, reason} ->
        :persistent_term.erase({__MODULE__.Plug.Server, :pid})
        {:stop, reason}
    end
  end

  defp start_bandit do
    case Bandit.start_link(plug: AdoCli.TestServer.Plug, port: 0) do
      {:ok, pid} ->
        port = get_port(pid)
        {:ok, pid, port}

      other ->
        other
    end
  end

  defp get_port(bandit_pid) do
    # Bandit is a supervisor with a ThousandIsland child. After
    # start_link returns, the listener is bound. We can ask the
    # supervisor for its children, or just read the port from the
    # underlying listener. Easiest: wait briefly and ask Bandit.
    :timer.sleep(20)
    {_, {_ip, port}} = ThousandIsland.listener_info(bandit_pid)
    port
  rescue
    _ -> 0
  end

  @impl true
  def handle_call({:expect, method, path, handler}, _from, state) do
    queue = [{method, path, handler} | state.expectations]
    {:reply, :ok, %{state | expectations: queue}}
  end

  def handle_call(:count, _from, state) do
    {:reply, length(state.expectations), state}
  end

  def handle_call({:pop, method, path}, _from, state) do
    case Enum.find(state.expectations, &match?({^method, ^path, _}, &1)) do
      {^method, ^path, handler} ->
        queue = List.delete(state.expectations, {method, path, handler})
        {:reply, {:ok, handler}, %{state | expectations: queue}}

      nil ->
        {:reply, :error, state}
    end
  end

  @impl true
  def handle_info(_msg, state), do: {:noreply, state}

  @impl true
  def terminate(_reason, state) do
    :persistent_term.erase({__MODULE__.Plug.Server, :pid})

    if state.bandit_pid && Process.alive?(state.bandit_pid) do
      Process.exit(state.bandit_pid, :shutdown)
    end

    :ok
  end
end
