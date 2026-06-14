defmodule AdoCli.CI.Watcher do
  @moduledoc """
  Streams live status and log output for an Azure DevOps build.

  Uses three Azure DevOps Build API endpoints:

    1. `GET .../build/builds/{id}` — build metadata (status, result, source branch, etc.)
    2. `GET .../build/builds/{id}/timeline` — list of records (jobs, tasks, phases)
       with their current state. The timeline is the source of truth for
       "which step is currently running".
    3. `GET .../build/builds/{id}/logs/{logId}?id={N}` — log content up to line `N`.
       The `?id=N` parameter is the streaming primitive: incrementing `N` and
       re-fetching gives us only the new lines.

  The watcher runs a single GenServer-like polling loop. On every tick it:

    1. Fetches the build status (to know if we should keep watching)
    2. Fetches the timeline (to know which logs to stream)
    3. For each log that is currently `inProgress` and has a `log.id`,
       fetches the new content with `?id=<last_seen_line>` and prints it
    4. Sleeps `poll_ms` and repeats

  Ctrl+C cancels the watch via a flag in process dictionary
  (`:ado_cli_watch_cancel`), so the loop can exit cleanly.
  """

  alias AdoCli.Client

  @type build :: map()
  @type timeline_record :: map()
  @type log_progress :: %{optional(integer()) => non_neg_integer()}

  @doc """
  Watches a build until it reaches a terminal state. Returns
  `:ok` on success, `{:error, reason}` otherwise.

  ## Options

    * `:poll_ms` — how often to poll (default 2000ms, min 250ms)
    * `:print_callback` — function for output (default: &IO.write/2).
      Tests inject a collector here.
  """
  @spec watch(integer(), String.t(), String.t() | nil, keyword()) ::
          :ok | {:error, term()}
  def watch(build_id, project, org, opts \\ []) do
    poll_ms = max(Keyword.get(opts, :poll_ms, 2000), 250)
    print = Keyword.get(opts, :print_callback, &IO.write/2)

    state = %{
      build_id: build_id,
      project: project,
      org: org,
      last_log_lines: %{},
      last_status: nil,
      last_timeline: [],
      started_at: System.monotonic_time(:millisecond)
    }

    Process.put(:ado_cli_watch_cancel, false)
    # Install Ctrl+C handler once per watch
    original_handler = Process.flag(:trap_exit, false)

    try do
      loop(state, poll_ms, print)
    catch
      :throw, :cancel -> :ok
    after
      Process.flag(:trap_exit, original_handler)
      Process.delete(:ado_cli_watch_cancel)
    end
  end

  # ── main loop ────────────────────────────────────────────────────────

  defp loop(state, poll_ms, print) do
    if Process.get(:ado_cli_watch_cancel, false) do
      throw(:cancel)
    end

    case fetch_build(state.build_id, state.project, state.org) do
      {:ok, build} ->
        state = %{state | last_status: build}
        render_status(build, state.started_at, print)
        render_timeline_diff(state, print)

        if terminal?(build) do
          render_final(build, print)
          :ok
        else
          state = stream_active_logs(state, print)
          Process.sleep(poll_ms)
          loop(state, poll_ms, print)
        end

      {:error, reason} ->
        {:error, reason}
    end
  end

  # ── status rendering ────────────────────────────────────────────────

  defp render_status(build, started_at, print) do
    elapsed_ms = System.monotonic_time(:millisecond) - started_at
    elapsed = format_duration(elapsed_ms)

    line =
      case build do
        %{"status" => "inProgress", "result" => nil} ->
          "* Build #{build["id"]} · #{build["definition"]["name"]} · #{build["sourceBranch"]} · running for #{elapsed}"

        %{"status" => "completed", "result" => result} ->
          "✓ Build #{build["id"]} · #{build["definition"]["name"]} · #{build["sourceBranch"]} · #{result} in #{elapsed}"

        %{"status" => "cancelling"} ->
          "* Build #{build["id"]} · cancelling… (#{elapsed})"

        %{"status" => "postponed"} ->
          "* Build #{build["id"]} · postponed (waiting for resources)"

        other ->
          "? Build #{other["id"]} · status=#{other["status"]} result=#{inspect(other["result"])} · #{elapsed}"
      end

    print.(line <> "\n")
  end

  defp render_final(build, print) do
    case build do
      %{"result" => "succeeded"} ->
        print.("\n  Build succeeded.\n")

      %{"result" => "partiallySucceeded"} ->
        print.("\n  Build partially succeeded (some warnings).\n")

      %{"result" => "failed"} ->
        print.("\n  Build failed.\n")

      %{"result" => "canceled"} ->
        print.("\n  Build was canceled.\n")

      _ ->
        :ok
    end
  end

  # ── timeline rendering ──────────────────────────────────────────────

  # Compares the new timeline with the last one and prints new records.
  # Each record represents a job, task, or phase in the pipeline.
  defp render_timeline_diff(state, print) do
    case fetch_timeline(state.build_id, state.project, state.org) do
      {:ok, records} ->
        Enum.each(new_records(state.last_timeline, records), fn rec ->
          print_record(rec, print)
        end)

        %{state | last_timeline: records}

      {:error, _} ->
        state
    end
  end

  defp new_records(old, new) do
    old_ids = MapSet.new(old, & &1["id"])
    Enum.reject(new, fn r -> MapSet.member?(old_ids, r["id"]) end)
  end

  defp print_record(rec, print) do
    icon = record_icon(rec["state"])
    name = rec["name"] || rec["type"] || "?"
    type = rec["type"] || "record"
    print.("  #{icon} [#{type}] #{name}\n")
  end

  defp record_icon("completed"), do: "  ✔"
  defp record_icon("inProgress"), do: "  *"
  defp record_icon("failed"), do: "  ✗"
  defp record_icon("skipped"), do: "  —"
  defp record_icon(_), do: "  ·"

  # ── log streaming ───────────────────────────────────────────────────

  # Fetches the timeline, picks the records that are currently in
  # progress and have a log, and streams new log content for each.
  defp stream_active_logs(state, print) do
    case fetch_timeline(state.build_id, state.project, state.org) do
      {:ok, records} ->
        records
        |> Enum.filter(fn r -> r["state"] == "inProgress" and r["log"] end)
        |> Enum.reduce(state, fn rec, state ->
          log_id = rec["log"]["id"]
          last = Map.get(state.last_log_lines, log_id, 0)
          stream_log(state, log_id, last, print)
        end)

      {:error, _} ->
        state
    end
  end

  defp stream_log(state, log_id, last_line, print) do
    # The `?id=N` parameter returns up to N lines. We pass
    # `last_line + 1` so the response contains only NEW lines.
    # We use `get_raw` (returns the body as a binary) so we can
    # print it directly to stdout without JSON parsing.
    path = "/_apis/build/builds/#{state.build_id}/logs/#{log_id}"
    params = %{"id" => last_line + 1}

    case Client.get_raw(path, params) do
      {:ok, ""} ->
        %{state | last_log_lines: Map.put_new(state.last_log_lines, log_id, last_line)}

      {:ok, content} ->
        # Azure DevOps sends content as a string with `\r\n` line endings.
        # Normalize to `\n` for clean terminal display.
        normalized = String.replace(content, "\r\n", "\n")
        print.(normalized)
        new_last = last_line + count_newlines(content)
        %{state | last_log_lines: Map.put(state.last_log_lines, log_id, new_last)}

      {:error, _} ->
        # Log not available yet (e.g. record just started). Keep
        # the last_line as-is and try again next tick.
        %{state | last_log_lines: Map.put_new(state.last_log_lines, log_id, last_line)}
    end
  end

  defp count_newlines(content) do
    content
    |> String.split(["\r\n", "\n"], trim: true)
    |> length()
  end

  # ── API fetches ─────────────────────────────────────────────────────

  defp fetch_build(build_id, _project, _org) do
    Client.get("/_apis/build/builds/#{build_id}")
  end

  defp fetch_timeline(build_id, _project, _org) do
    Client.get("/_apis/build/builds/#{build_id}/timeline")
  end

  # ── formatting ──────────────────────────────────────────────────────

  defp format_duration(ms) when ms < 1000, do: "<1s"

  defp format_duration(ms) do
    seconds = div(ms, 1000)

    cond do
      seconds < 60 -> "#{seconds}s"
      seconds < 3600 -> "#{div(seconds, 60)}m#{rem(seconds, 60)}s"
      true -> "#{div(seconds, 3600)}h#{div(rem(seconds, 3600), 60)}m"
    end
  end

  # Detect "completed" or "cancelling" — both are terminal-ish.
  # cancelled is the post-cancel state; cancelling is in flight.
  defp terminal?(%{"status" => "completed"}), do: true
  defp terminal?(%{"status" => "cancelling"}), do: true
  defp terminal?(_), do: false
end
