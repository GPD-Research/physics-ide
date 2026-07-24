export function getModelOptions(provider) {
  switch (provider) {
    case 'gemini':
      return [
        { value: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash (Fast / Balanced)' },
        { value: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash (Fallback)' },
      ];
    case 'openai':
      return [
        { value: 'gpt-4o-mini', label: 'GPT-4o Mini (Light / Fast)' },
        { value: 'gpt-4o', label: 'GPT-4o Flagship (Heavy / Data Crunching)' },
      ];
    case 'ollama':
      return [
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
