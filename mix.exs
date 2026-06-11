defmodule AdoCli.MixProject do
  use Mix.Project

  def project do
    [
      app: :ado_cli,
      version: "0.1.0",
      elixir: "~> 1.20",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      releases: releases(),
      escript: escript_config(),
      aliases: aliases(),
      dialyzer: [
        plt_file: {:no_warn, "priv/plts/dialyzer.plt"},
        plt_add_apps: [:cli_mate, :burrito, :finch, :mint, :ex_unit],
        flags: [:error_handling, :missing_return, :extra_return]
      ],
      docs: [
        main: "AdoCli",
        source_url: "https://github.com/your-org/ado_cli",
        extras: ["README.md", "USAGE.md", "AUTH.md"]
      ]
    ]
  end

  def application do
    if Mix.env() == :prod do
      [mod: {AdoCli.Application, []}, extra_applications: [:logger]]
    else
      [extra_applications: [:logger]]
    end
  end

  defp deps do
    [
      {:finch, "~> 0.22.0"},
      {:cli_mate, "~> 0.10.2"},
      {:burrito,
       github: "gilbertwong96/burrito",
       ref: "b08d236ff54b012e6c2c4eb0fd800cd78450347a",
       runtime: false},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:ex_dna, "~> 1.5", only: [:dev, :test], runtime: false},
      {:ex_slop, "~> 0.4.2", only: [:dev, :test], runtime: false},
      {:reach, "~> 2.7", only: [:dev, :test], runtime: false},
      {:mix_audit, "~> 2.1", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false},
      {:ex_doc, "~> 0.40", only: :dev, runtime: false},
      {:pi_bridge, "~> 0.6.5", only: [:dev, :test], runtime: false}
    ]
  end

  defp aliases do
    [
      ci: [
        "compile --all-warnings --warnings-as-errors",
        "format --check-formatted",
        "credo --strict",
        "deps.unlock --check-unused",
        "deps.audit",
        "xref graph --label compile-connected --fail-above 0",
        "ci.dialyzer"
      ],
      quality: [
        "compile --all-warnings --warnings-as-errors",
        "credo --strict",
        "ex_dna",
        "reach.check --dead-code --smells",
        "test"
      ],
      lint: ["credo --strict"],
      inspect: ["reach.map"],
      health: ["reach.check --dead-code --smells"]
    ]
  end

  defp escript_config do
    [
      main_module: AdoCli.CLI,
      name: "ado",
      app: nil,
      emu_args: "-noshell -elixir ansi_enabled true"
    ]
  end

  # Burrito-wrapped native binaries for distribution.
  # Only available in prod — dev uses `mix escript.build` for CLI iteration.
  defp releases do
    if Mix.env() == :prod do
      [
        ado: [
          steps: [:assemble, &Burrito.wrap/1],
          burrito: [
            targets: [
              macos: [os: :darwin, cpu: :aarch64],
              linux: [os: :linux, cpu: :x86_64],
              linux_arm: [os: :linux, cpu: :aarch64],
              windows: [os: :windows, cpu: :x86_64],
              windows_arm: [os: :windows, cpu: :aarch64]
            ]
          ]
        ]
      ]
    else
      []
    end
  end
end
