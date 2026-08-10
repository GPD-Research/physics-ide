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
