// Крайне упрощённый маркдаун: экранируем HTML, затем поддерживаем
// **bold**, *italic* и переносы строк. Никакого HTML от модели не выполняется.

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

export function renderLiteMarkdown(text) {
    let safe = escapeHtml(text);

    // **bold**
    safe = safe.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    // *italic* (после bold, чтобы не конфликтовать с двойными звёздочками)
    safe = safe.replace(/\*(.+?)\*/g, '<em>$1</em>');
    // переносы строк
    safe = safe.replace(/\n/g, '<br>');

    return safe;
}