const SETTINGS_KEY = 'wrp_settings';

const DEFAULT_SETTINGS = {
    responseLength: 'medium', // short | medium | long
    typingEffect: true,       // simulate live typing on frontend
};

export function getSettings() {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    try {
        return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
    } catch {
        return { ...DEFAULT_SETTINGS };
    }
}

export function saveSettings(settings) {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}