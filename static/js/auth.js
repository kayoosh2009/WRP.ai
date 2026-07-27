// Общая логика Firebase Auth + кэш пользователя в localStorage.
// Импортируется как модуль на всех страницах, где нужен вход.

import { initializeApp } from "https://www.gstatic.com/firebasejs/10.12.2/firebase-app.js";
import {
    getAuth,
    GoogleAuthProvider,
    signInWithPopup,
    signOut as fbSignOut,
} from "https://www.gstatic.com/firebasejs/10.12.2/firebase-auth.js";

const STORAGE_KEY = "wrp_auth";

let app, auth;

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
    if (!auth.currentUser) {
        const cached = getCachedUser();
        return cached ? cached.idToken : null;
    }
    const token = await auth.currentUser.getIdToken();
    const cached = getCachedUser();
    if (cached) {
        cached.idToken = token;
        localStorage.setItem(STORAGE_KEY, JSON.stringify(cached));
    }
    return token;
}

// Редирект на /login.html, если пользователь не авторизован
export function requireAuthOrRedirect() {
    const user = getCachedUser();
    if (!user) {
        window.location.href = "/login.html";
        return null;
    }
    return user;
}

initFirebase().catch((e) => console.error("Firebase init error:", e));