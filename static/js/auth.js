// Общая логика Firebase Auth + кэш пользователя в localStorage.
// Импортируется как модуль на всех страницах, где нужен вход.

import { initializeApp } from "https://www.gstatic.com/firebasejs/10.12.2/firebase-app.js";
import {
    getAuth,
    GoogleAuthProvider,
    signInWithPopup,
    signOut as fbSignOut,
    onAuthStateChanged,
} from "https://www.gstatic.com/firebasejs/10.12.2/firebase-auth.js";

const STORAGE_KEY = "wrp_auth";

let app, auth;
let authReadyResolve;
const authReady = new Promise((resolve) => { authReadyResolve = resolve; });

async function initFirebase() {
    if (app) return app;
    const res = await fetch("/api/firebase-config");
    if (!res.ok) throw new Error("Не удалось загрузить конфиг Firebase");
    const cfg = await res.json();

    app = initializeApp({
        apiKey: cfg.api_key,
        authDomain: cfg.auth_domain,
        projectId: cfg.project_id,
        storageBucket: cfg.storage_bucket,
        messagingSenderId: cfg.messaging_sender_id,
        appId: cfg.app_id,
    });
    auth = getAuth(app);

    // Ждём первого срабатывания onAuthStateChanged — это значит,
    // что Firebase закончил восстановление сессии из своего хранилища.
    // Только после этого auth.currentUser можно доверять.
    const unsubscribe = onAuthStateChanged(auth, () => {
        authReadyResolve();
        unsubscribe();
    });

    return app;
}

export function getCachedUser() {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    try {
        return JSON.parse(raw);
    } catch {
        return null;
    }
}

function cacheUser(idToken, profile) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ idToken, profile, savedAt: Date.now() }));
}

export function clearCachedUser() {
    localStorage.removeItem(STORAGE_KEY);
}

export async function signInWithGoogle() {
    await initFirebase();
    const provider = new GoogleAuthProvider();
    const result = await signInWithPopup(auth, provider);
    const idToken = await result.user.getIdToken();
    const profile = {
        uid: result.user.uid,
        name: result.user.displayName,
        email: result.user.email,
        picture: result.user.photoURL,
    };
    cacheUser(idToken, profile);
    return profile;
}

export async function signOutUser() {
    await initFirebase();
    try {
        await fbSignOut(auth);
    } catch (e) {
        console.warn("Firebase signOut error:", e);
    }
    clearCachedUser();
}

// Возвращает актуальный idToken (Firebase сам обновит его при необходимости)
export async function getFreshIdToken() {
    await initFirebase();
    await authReady; // дожидаемся восстановления сессии Firebase после перезагрузки страницы

    if (!auth.currentUser) {
        // Сессия правда не восстановилась (например, реально разлогинен) —
        // чистим протухший кэш, чтобы не гонять по кругу с невалидным токеном
        clearCachedUser();
        return null;
    }

    const token = await auth.currentUser.getIdToken(); // Firebase сам обновит, если истёк
    const cached = getCachedUser();
    if (cached) {
        cached.idToken = token;
        localStorage.setItem(STORAGE_KEY, JSON.stringify(cached));
    }
    return token;
}

// Редирект на /login.html, если пользователь не авторизован.
// Возвращает кэшированный профиль синхронно (для мгновенного рендера),
// но дополнительно проверяет актуальность сессии в фоне.
export function requireAuthOrRedirect() {
    const cached = getCachedUser();
    if (!cached) {
        window.location.href = "/login.html";
        return null;
    }

    // Асинхронно подтверждаем, что сессия Firebase реально жива.
    // Если нет — редиректим постфактум (страница уже могла отрендериться,
    // но следующий защищённый запрос всё равно был бы отклонён с 401).
    (async () => {
        await initFirebase();
        await authReady;
        if (!auth.currentUser) {
            clearCachedUser();
            window.location.href = "/login.html";
        }
    })();

    return cached;
}

initFirebase().catch((e) => console.error("Firebase init error:", e));