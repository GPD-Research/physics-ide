export function getModelOptions(provider) {
  switch (provider) {
    case 'gemini':
      return [
        { value: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash (Fast / Balanced)' },
        { value: 'gemini-2.0-flash-lite', label: 'Gemini 2.0 Flash Lite (Light / Efficient)' },
      ];
    case 'openai':
      return [
        { value: 'gpt-4o-mini', label: 'GPT-4o Mini (Light / Fast)' },
        { value: 'gpt-4o', label: 'GPT-4o Flagship (Heavy / Data Crunching)' },
      ];
    case 'ollama':
      return [
        { value: 'deepseek-r1:7b', label: 'DeepSeek R1 7B (Analytical / Thoughtful, ~4-5GB)' },
        { value: 'qwen2.5:7b', label: 'Qwen 2.5 7B (Conversational / Synthesis, ~4-5GB)' },
        { value: 'llama3:8b', label: 'Llama 3 8B (Light / Fast Local)' },
        { value: 'llama3:70b', label: 'Llama 3 70B / Mistral Large (Heavy Local)' },
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
      return 'Ollama';
    default:
      return 'Provider';
  }
}
