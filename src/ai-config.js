export function getModelOptions(provider) {
  switch (provider) {
    case 'gemini':
      return [
        { value: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash (Fast / Balanced)' },
        { value: 'gemini-2.0-flash-lite', label: 'Gemini 2.0 Flash Lite (Light / Efficient)' },
      ];
    case 'openai':
      return [
        { value: 'gpt-4.1-mini', label: 'GPT-4.1 Mini (Fast / Cost-Efficient)' },
        { value: 'gpt-4.1', label: 'GPT-4.1 Flagship (Heavy / Data Crunching)' },
        { value: 'gpt-4o-mini-2024-07-18', label: 'GPT-4o Mini 2024-07-18 (Light / Fast)' },
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
    default:
      return 'Provider';
  }
}
