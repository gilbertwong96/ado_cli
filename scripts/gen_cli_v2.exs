#!/usr/bin/env elixir
# Smarter generator that introspects each function's actual arguments.
# Run: mix run scripts/gen_cli_v2.exs

defmodule GenCliV2 do
  @moduledoc """
  Generates test files by introspecting each CLI function's actual
  arguments using `Code.fetch_docs/1` and AST analysis.
  """
  @test_dir "test/ado_cli/cli"

  def run do
    File.mkdir_p!(@test_dir)

    for {module, fns} <- specs(), {module, _fns} <- ... do
      # placeholder
      :ok
    end

    # Simple per-function approach: read the source, find the function
    # definition, extract the destructuring of parsed.arguments and the
    # Map.fetch! / Map.get calls on parsed.options.
    Path.wildcard("lib/ado_cli/cli/*.ex")
    |> Enum.reject(&String.contains?(&1, "helpers.ex"))
    |> Enum.map(&module_name/1)
    |> Enum.each(&generate/1)
  end

  defp module_name(path) do
    path
    |> Path.basename(".ex")
    |> String.split("_")
    |> Enum.map_join(".", &String.capitalize/1)
    |> then(&("AdoCli.CLI." <> &1))
  end

  defp generate(module) do
    file = "lib/ado_cli/cli/#{macro_name(module)}.ex"
    source = File.read!(file)

    case analyze_module(source) do
      :none ->
        :ok

      functions ->
        test_file = "#{@test_dir}/#{macro_name(module)}_test.exs"
        if File.exists?(test_file), do: :ok, else: do_generate(module, functions, test_file)
    end
  end

  defp analyze_module(source) do
    # Find all `def NAME(parsed)` functions and extract the body
    regex = ~r/def\s+(\w+)\s*\(parsed\)\s+do(.*?)(?=\n\s*def\s|\n\s*defp\s|\n\s+end\s*$\n|\Z)/m

    Regex.scan(regex, source, capture: :all_but_first)
    |> Enum.map(fn [name, body] -> {name, analyze_body(body)} end)
  end

  # Parse the function body to find argument keys and HTTP calls
  defp analyze_body(body) do
    args = extract_args(body)
    options = extract_options(body)
    http_call = detect_http_call(body)
    %{args: args, options: options, http_call: http_call}
  end

  defp extract_args(body) do
    # Find patterns like:
    #   %{project: project, repo_id: repo_id, pr_id: pr_id} = parsed.arguments
    #   project = parsed.arguments.project
    #   %{name: name} = parsed.arguments
    regex = ~r/parsed\.arguments(?:\.(\w+))?(?:\s*do|\s+as\s+\{([^}]+)\})?/

    case Regex.run(regex, body, capture: :all_but_first) do
      [_, nil, nil] -> []
      [_, key, nil] -> [key]
      [_, nil, struct] -> parse_struct_keys(struct)
      [_, key, struct] when is_binary(key) and is_binary(struct) -> [key | parse_struct_keys(struct)]
      _ -> []
    end
  end

  defp parse_struct_keys(struct) do
    # %{a: x, b: y} -> [a, b]
    Regex.scan(~r/(\w+):/, struct, capture: :all_but_first)
    |> Enum.map(fn [k] -> k end)
  end

  defp extract_options(body) do
    # Find patterns like:
    #   value = Map.get(parsed.options, :name)
    #   value = Map.fetch!(parsed.options, :name)
    regex1 = ~r/Map\.(?:get|fetch!)\(parsed\.options,\s*:(\w+)\)/

    Regex.scan(regex1, body, capture: :all_but_first)
    |> Enum.map(fn [k] -> k end)
  end

  defp detect_http_call(body) do
    cond do
      String.contains?(body, "Client.post(") -> :post
      String.contains?(body, "Client.put(") -> :put
      String.contains?(body, "Client.patch(") -> :patch
      String.contains?(body, "Client.delete(") -> :delete
      String.contains?(body, "Client.get(") -> :get
      true -> nil
    end
  end

  defp do_generate(module, functions, test_file) do
    content = render(module, functions)
    File.write!(test_file, content)
    IO.puts("  Generated #{Path.basename(test_file)} (#{length(functions)} functions)")
  end

  defp render(module, functions) do
    tests = Enum.map_join(functions, "\n\n", fn {name, info} -> render_test(module, name, info) end)

    """
    defmodule #{module}Test do
      use AdoCli.CLI.TestHelper
      alias #{module}

    #{tests}
    end
    """
  end

  defp render_test(module, fn_name, %{args: args, options: options, http_call: method}) do
    args_map =
      if args == [] do
        ""
      else
        pairs = Enum.map_join(args, ", ", fn k -> "#{k}: 1" end)
        ", arguments: %{#{pairs}}"
      end

    options_map = build_options_map(options)

    # Try to extract the path from the body
    path = extract_path(method, fn_name)

    cond do
      method == nil ->
        # Non-HTTP function (e.g., reads config)
        """
            test "#{fn_name} works" do
              apply(#{module}, :#{fn_name}, [%{options: %{json: true, #{options_map}}#{args_map}}])
              assert_receive {:cli_mate_shell, :halt, _}, 500
            end
        """

      true ->
        method_str =
          case method do
            :get -> "expect_success_json"
            :post -> "expect_post_success"
            :put -> "expect_put_success"
            :patch -> "expect_patch_success"
            :delete -> "expect_delete_success"
          end

        body_arg =
          case method do
            :get -> ", ~s({\"value\":[]})"
            m when m in [:post, :put, :patch] -> ", \"\", \"{\\\"id\\\":1}\""
            :delete -> ""
          end

        """
            describe "#{fn_name}" do
              test "halts 0 on success", %{server: server} do
                #{method_str}(server, "#{path}"#{body_arg}, fn ->
                  apply(#{module}, :#{fn_name}, [%{options: %{#{options_map}}#{args_map}}])
                end)
              end

              test "halts 1 on API error", %{server: server} do
                expect_api_error(server, "#{path}", 500, "{}", fn ->
                  apply(#{module}, :#{fn_name}, [%{options: %{#{options_map}}#{args_map}}])
                end)
              end
            end
        """
    end
  end

  defp build_options_map(options) do
    base = "json: true"
    extras = Enum.map_join(options, ", ", fn k -> "#{k}: default_value_for(#{k})" end)
    if extras == "", do: base, else: base <> ", " <> extras
  end

  # Default values for known option keys
  defp default_value_for("name"), do: "\"test\""
  defp default_value_for("description"), do: "\"test\""
  defp default_value_for("title"), do: "\"test\""
  defp default_value_for("type"), do: "\"Bug\""
  defp default_value_for("text"), do: "\"comment\""
  defp default_value_for("state"), do: "\"Active\""
  defp default_value_for("branch"), do: "\"main\""
  defp default_value_for("path"), do: "\"test.yml\""
  defp default_value_for("message"), do: "\"test\""
  defp default_value_for("wiql"), do: "\"SELECT [System.Id] FROM WorkItems\""
  defp default_value_for("variables"), do: "nil"
  defp default_value_for("tags"), do: "nil"
  defp default_value_for("source"), do: "\"github\""
  defp default_value_for("endpoint"), do: "\"https://api.github.com\""
  defp default_value_for("repository"), do: "\"repo\""
  defp default_value_for(_), do: "1"

  # Heuristic path extraction
  defp extract_path(method, fn_name) do
    # For now, just use a placeholder - we'd need to read the actual file
    "/_apis/#{function_to_resource(fn_name)}"
  end

  defp function_to_resource("list_areas"), do: "wit/classificationnodes"
  defp function_to_resource("list_pools"), do: "distributedtask/pools"
  defp function_to_resource("list_pipelines"), do: "pipelines"
  defp function_to_resource("list_queues"), do: "distributedtask/queues"
  defp function_to_resource("list_groups"), do: "graph/groups"
  defp function_to_resource("list_repos"), do: "git/repositories"
  defp function_to_resource("list_branches"), do: "git/refs"
  defp function_to_resource("list_artifacts"), do: "pipelines/artifacts"
  defp function_to_resource("list_imports"), do: "git/importRequests"
  defp function_to_resource(_), do: "resource"
end

GenCliV2.run()
