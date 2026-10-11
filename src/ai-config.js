export function getModelOptions(provider) {
  switch (provider) {
    case 'gemini':
      return [
        { value: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash (Fast / Balanced)' },
        { value: 'gemini-2.5-flash-lite', label: 'Gemini 2.5 Flash Lite (Light / Efficient)' },
        { value: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro (High-Capability)' },
      ];
    case 'openai':
      return [
        { value: 'gpt-4.1-mini', label: 'GPT-4.1 Mini (Fast / Cost-Efficient)' },
        { value: 'gpt-4.1', label: 'GPT-4.1 Flagship (Heavy / Data Crunching)' },
        { value: 'gpt-4o-mini-2024-07-18', label: 'GPT-4o Mini 2024-07-18 (Light / Fast)' },
      ];
    case 'ollama':
      return [
        { value: 'llama3.1:8b', label: 'Llama 3.1 8B (Local / Balanced)' },
        { value: 'qwen2.5:7b', label: 'Qwen 2.5 7B (Local / Math-friendly)' },
        { value: 'qwen2.5:14b', label: 'Qwen 2.5 14B (Local / Deeper)' },
        { value: 'deepseek-r1:14b', label: 'DeepSeek R1 14B (Local / Reasoning)' },
      ];
    default:
      return [];
  }
}

export function getProviderLabel(provider) {
  switch (provider) {
    case 'gemini':
      return 'Gemini';
    case 'openai':
      return 'OpenAI';
    case 'ollama':
      return 'Local AI';
    default:
      return 'Provider';
  }
}
