# Ollama Setup (Windows and Linux)

## Why this matters

Local models require a working Ollama runtime plus explicit model downloads.

## Windows setup

1. Install Ollama from the official installer.
2. Open terminal (PowerShell or cmd) and run:

```bash
ollama --version
```

3. Pull at least one model used by your lanes, for example:

```bash
ollama pull qwen2.5:7b
ollama pull deepseek-r1:7b
```

4. Start or verify the local server endpoint:

```bash
ollama list
```

## Linux setup

1. Install Ollama using the official Linux method.
2. Verify installation:

```bash
ollama --version
```

3. Pull lane models:

```bash
ollama pull qwen2.5:7b
ollama pull deepseek-r1:7b
```

4. Verify models are available:

```bash
ollama list
```

## Configure physics-IDE

1. Open Customize menu.
2. In provider settings, set Ollama URL to:
- http://127.0.0.1:11434
3. Pick models that exist in `ollama list` output.
4. Save settings and test both lanes.

## Troubleshooting

1. If model not found, pull it explicitly again.
2. If connection fails, confirm Ollama server is running and URL is correct.
3. If performance is slow, test a smaller model first.

## Maintenance note

Model tags and recommended defaults evolve. Keep this guide and default model list synchronized with current provider releases.
