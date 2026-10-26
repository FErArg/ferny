// FernyEditor: Ferny's editing widget, a thin typed layer over Scintilla's
// Qt platform widget (ScintillaEditBase). Using the base widget avoids the
// generated ScintillaEdit wrapper, whose sources need a Python code
// generation step.
#pragma once

#include <QByteArray>
#include <QList>
#include <QPair>
#include <QString>
#include <QStringList>

#include <ScintillaEditBase.h>

#include <functional>
#include <optional>

namespace ferny {

// One resolved style; colours are 0xRRGGBB.
struct StyleLook {
    int style = 0;
    quint32 fore = 0x000000;
    quint32 back = 0xffffff;
    bool bold = false;
    bool italic = false;
    bool fillEol = false;
};

// Everything needed to highlight a document: the Lexilla lexer with its
// keywords and properties, the lexer styles and the editor colours of the
// colour scheme. Built from the core's filetypes and schemes.
struct Highlighting {
    QByteArray lexer;  // empty = plain text
    QList<QByteArray> keywords;
    QList<QPair<QByteArray, QByteArray>> properties;
    StyleLook defaults;
    QList<StyleLook> styles;
    std::optional<StyleLook> caret, selection, currentLine, lineNumbers, braceGood, braceBad,
        indentGuide, whiteSpace;
};

enum class VirtualSpace { Off, Rectangular, Always };
enum class ChangeHistory { Off, Margin, Indicator };

// Display options from the editor.* preferences (M1). The defaults match the
// behaviour before these options existed.
struct DisplayOptions {
    bool wordWrap = false;
    bool indentGuides = false;
    bool showEol = false;
    bool markersMargin = true;
    bool folding = true;
    bool scrollPastEnd = false;
    int caretMarginLines = 0;
    bool longLineBackground = false;
    VirtualSpace virtualSpace = VirtualSpace::Off;
    ChangeHistory changeHistory = ChangeHistory::Off;
};

// Editing behaviour from the editor.* preferences (M2). Defaults match the
// behaviour before these options existed.
struct EditingOptions {
    bool autoIndent = true;
    bool tabIndents = true;
    bool backspaceUnindents = false;
    bool smartHome = true;
    bool newlineStripsTrailing = false;
    bool autoCloseQuotes = false;
};

class FernyEditor : public ScintillaEditBase {
    Q_OBJECT

public:
    explicit FernyEditor(QWidget *parent = nullptr);

    // Replaces the document with `utf8` (may contain NUL bytes) and resets
    // undo history and the save point.
    void setUtf8Text(const QByteArray &utf8);

    // Applies a lexer, its styles and the scheme's editor colours.
    void applyHighlighting(const Highlighting &highlighting);

    // Filetype id shown in this editor (empty = plain text).
    QString filetypeId() const { return m_filetypeId; }
    void setFiletypeId(const QString &id) { m_filetypeId = id; }

    // Background of the default style (0xRRGGBB), for tests.
    quint32 defaultBackground() const;

    // Columns per indentation level and tabs or spaces.
    void setIndentation(int width, bool useTabs);

    // Font family (empty = system fixed-width) and size in points.
    void setFontSettings(const QString &family, int size) {
        m_fontFamily = family;
        m_fontSize = size;
    }

    // Called on Tab without modifiers or selection; returning true means
    // the key was used (snippets, H2.5b).
    void setTabHandler(std::function<bool()> handler) { m_tabHandler = std::move(handler); }

    // Replaces the `before` bytes before the cursor with `text` as one
    // undo action and puts the cursor `cursor` characters into it (-1:
    // at its end).
    void replaceBeforeCursor(int before, const QString &text, int cursor);

    // Replaces the whole text with `utf8` as one undo action, keeping the
    // cursor's line (H3.1c: rename, formatting).
    void replaceAllText(const QByteArray &utf8);

    // Leading whitespace of the cursor's line.
    QString lineIndentation() const;

    // Auto-close of (), {} and [] (on by default).
    void setAutoClose(bool enabled) { m_autoClose = enabled; }
    // Applies the display options; see fernyeditor.cpp for the change-history rule.
    void applyDisplay(const DisplayOptions &options);
    void setEditingOptions(const EditingOptions &options);
    // Zoom is session state of this editor, not a preference.
    void zoomIn();
    void zoomOut();
    void resetZoom();

    // Draws the given characters as labels (alternating character, label)
    // or, with an empty list, as Scintilla does by default (T7).
    void setControlCharacterLabels(const QStringList &labels);

    // Markers (bookmarks) on the current line.
    void toggleMarker();
    // Moves to the next (or previous) marked line, wrapping around.
    void gotoMarker(bool forward);

    // Replaces the whole lines of the selection with `transform(text)`
    // (lines without the final line ending) as one undo action.
    template <typename Transform>
    bool transformSelectedLines(Transform transform);

    // The whole lines covered by the selection.
    QString selectedLinesText() const;
    void replaceSelectedLines(const QString &text);

    // Mark word (Geany's addons, H4.2a): every whole-word, same-case
    // occurrence of the selection or the word at the cursor; returns how many.
    int markOccurrences();
    void clearMarks();

    // The selected text (main selection).
    QString selectedText() const;
    // Plugin edits (H3.3a): `kind, text` pairs (`insert` at the cursor,
    // `replace_selection`) applied as one undo action.
    void applyPluginEdits(const QStringList &edits);

    // Name of the active lexer, or an empty string for plain text.
    QString lexerName() const;

    // The whole document as UTF-8 (may contain NUL bytes).
    QByteArray utf8Text() const;

    // Whether the text changed since the last save point.
    bool isModified() const;

    // Marks the current text as saved.
    void setSavePoint();

    void setReadOnly(bool readOnly);
    bool isReadOnly() const { return send(SCI_GETREADONLY) != 0; }

    // Line ending for new lines (Scintilla SC_EOL_* value).
    void setEolMode(int mode);

    // Document length in bytes.
    qint64 length() const;

    // Moves the cursor to a 1-based line and column (clamped to the text)
    // and scrolls it into view. 0 means "first".
    void gotoPosition(quint32 line, quint32 column);

    // 1-based line and column of the cursor.
    quint32 cursorLine() const;
    quint32 cursorColumn() const;

    // The word around the cursor, or only its part before the cursor.
    QString wordAtCursor(bool beforeCursorOnly) const;

    // Shows Scintilla's completion list; `prefixLength` characters before
    // the cursor are replaced by the chosen word.
    void showCompletions(const QStringList &words, int prefixLength);

    // Number of lines in the document.
    qint64 lineCount() const;

    // Characters (not bytes) selected, over every selection.
    qint64 selectedCharacters() const;

    // Columns per indentation level, and whether indentation uses tabs.
    int indentWidth() const;
    bool indentUsesTabs() const;

    // Id of the core document shown in this editor (0 = none).
    quint64 documentId() const { return m_documentId; }
    void setDocumentId(quint64 id) { m_documentId = id; }

    // True if lexing assigned a non-default style anywhere in the first
    // `maxBytes` bytes (forces lexing of that range first).
    bool hasStyledText(qint64 maxBytes) const;

    // The text between two byte positions.
    QString textRange(sptr_t start, sptr_t end) const;

protected:
    void keyPressEvent(QKeyEvent *event) override;

private:
    void applyFont();
    void onCharAdded(int ch);
    void highlightBraces();
    void autoIndent();
    void autoCloseQuote(int quote);
    void stripTrailingSpaces(sptr_t line);

    quint64 m_documentId = 0;
    QString m_filetypeId;
    bool m_autoClose = true;
    bool m_autoIndent = true;
    bool m_newlineStripsTrailing = false;
    bool m_autoCloseQuotes = false;
    QString m_fontFamily;
    int m_fontSize = 12;
    int m_indentWidth = 4;
    std::function<bool()> m_tabHandler;
};

template <typename Transform>
bool FernyEditor::transformSelectedLines(Transform transform) {
    QString result;
    if (!transform(selectedLinesText(), result)) {
        return false;
    }
    replaceSelectedLines(result);
    return true;
}

}  // namespace ferny
