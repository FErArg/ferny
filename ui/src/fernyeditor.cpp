#include "fernyeditor.h"

#include <QColor>
#include <QFont>
#include <QFontDatabase>
#include <QKeyEvent>

#include <ILexer.h>
#include <Lexilla.h>
#include <Scintilla.h>

#include <algorithm>
#include <cctype>

namespace ferny {
namespace {

// Scintilla colours are 0xBBGGRR; Ferny's are 0xRRGGBB.
constexpr sptr_t bgr(quint32 rgb) {
    return static_cast<sptr_t>(((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff));
}

constexpr quint32 rgb(sptr_t bgr) {
    const auto v = static_cast<quint32>(bgr);
    return ((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff);
}

constexpr uptr_t kMarkerMargin = 1;
constexpr uptr_t kFoldMargin = 2;
constexpr uptr_t kBookmark = 0;
// Indicators 8-10 are the diagnostics (MainWindow).
constexpr uptr_t kMarkIndicator = 11;

bool isOpener(int ch) {
    return ch == '(' || ch == '[' || ch == '{';
}

char closerFor(int ch) {
    return ch == '(' ? ')' : ch == '[' ? ']' : '}';
}

}  // namespace

FernyEditor::FernyEditor(QWidget *parent) : ScintillaEditBase(parent) {
    send(SCI_SETCODEPAGE, SC_CP_UTF8);
    send(SCI_SETEOLMODE, SC_EOL_LF);
    send(SCI_SETTABWIDTH, 4);
    send(SCI_SETMARGINTYPEN, 0, SC_MARGIN_NUMBER);
    send(SCI_SETMARGINWIDTHN, 0, 48);

    // Margin 1: markers (bookmarks), click toggles. Margin 2: folding.
    send(SCI_SETMARGINTYPEN, kMarkerMargin, SC_MARGIN_SYMBOL);
    send(SCI_SETMARGINMASKN, kMarkerMargin, 1 << kBookmark);
    send(SCI_SETMARGINWIDTHN, kMarkerMargin, 16);
    send(SCI_SETMARGINSENSITIVEN, kMarkerMargin, 1);
    send(SCI_MARKERDEFINE, kBookmark, SC_MARK_BOOKMARK);
    send(SCI_MARKERSETBACK, kBookmark, bgr(0x3a7bd5));
    send(SCI_SETMARGINTYPEN, kFoldMargin, SC_MARGIN_SYMBOL);
    send(SCI_SETMARGINMASKN, kFoldMargin, SC_MASK_FOLDERS);
    send(SCI_SETMARGINWIDTHN, kFoldMargin, 14);
    send(SCI_SETMARGINSENSITIVEN, kFoldMargin, 1);
    const int folds[][2] = {{SC_MARKNUM_FOLDEROPEN, SC_MARK_BOXMINUS},
                            {SC_MARKNUM_FOLDER, SC_MARK_BOXPLUS},
                            {SC_MARKNUM_FOLDERSUB, SC_MARK_VLINE},
                            {SC_MARKNUM_FOLDERTAIL, SC_MARK_LCORNER},
                            {SC_MARKNUM_FOLDEREND, SC_MARK_BOXPLUSCONNECTED},
                            {SC_MARKNUM_FOLDEROPENMID, SC_MARK_BOXMINUSCONNECTED},
                            {SC_MARKNUM_FOLDERMIDTAIL, SC_MARK_TCORNER}};
    for (const auto &fold : folds) {
        send(SCI_MARKERDEFINE, static_cast<uptr_t>(fold[0]), fold[1]);
    }
    send(SCI_SETAUTOMATICFOLD,
         SC_AUTOMATICFOLD_SHOW | SC_AUTOMATICFOLD_CLICK | SC_AUTOMATICFOLD_CHANGE);

    // Multiple selections: Ctrl/Cmd+click adds a caret, Alt+drag selects a
    // rectangle; typing and pasting apply to every selection.
    send(SCI_SETMULTIPLESELECTION, 1);
    send(SCI_SETADDITIONALSELECTIONTYPING, 1);
    send(SCI_SETMULTIPASTE, SC_MULTIPASTE_EACH);
    send(SCI_SETRECTANGULARSELECTIONMODIFIER, SCMOD_ALT);

    connect(this, &ScintillaEditBase::charAdded, this, &FernyEditor::onCharAdded);
    connect(this, &ScintillaEditBase::updateUi, this,
            [this](Scintilla::Update) { highlightBraces(); });
    connect(this, &ScintillaEditBase::marginClicked, this,
            [this](Scintilla::Position position, Scintilla::KeyMod, int margin) {
                if (margin == kMarkerMargin) {
                    send(SCI_GOTOPOS, static_cast<uptr_t>(position));
                    toggleMarker();
                }
            });
    applyHighlighting(Highlighting{});
}

void FernyEditor::applyDisplay(const DisplayOptions &options) {
    send(SCI_SETWRAPMODE, options.wordWrap ? SC_WRAP_WORD : SC_WRAP_NONE);
    send(SCI_SETINDENTATIONGUIDES, options.indentGuides ? SC_IV_LOOKBOTH : SC_IV_NONE);
    send(SCI_SETVIEWEOL, options.showEol);
    send(SCI_SETENDATLASTLINE, !options.scrollPastEnd);
    send(SCI_SETYCARETPOLICY, CARET_SLOP | CARET_EVEN, options.caretMarginLines);
    send(SCI_SETEDGEMODE, options.longLineBackground ? EDGE_BACKGROUND : EDGE_LINE);

    send(SCI_SETMARGINWIDTHN, kMarkerMargin, options.markersMargin ? 16 : 0);
    // History markers live in the marker margin, so its mask includes them.
    uptr_t markerMask = 1u << kBookmark;
    if (options.changeHistory == ChangeHistory::Margin) {
        markerMask |= (1u << SC_MARKNUM_HISTORY_REVERTED_TO_ORIGIN) |
                      (1u << SC_MARKNUM_HISTORY_SAVED) | (1u << SC_MARKNUM_HISTORY_MODIFIED);
    }
    send(SCI_SETMARGINMASKN, kMarkerMargin, markerMask);

    // Hiding the fold margin expands every fold, so no line stays hidden.
    if (!options.folding) {
        send(SCI_FOLDALL, SC_FOLDACTION_EXPAND);
    }
    send(SCI_SETMARGINWIDTHN, kFoldMargin, options.folding ? 14 : 0);

    switch (options.virtualSpace) {
        case VirtualSpace::Off:
            send(SCI_SETVIRTUALSPACEOPTIONS, SCVS_NONE);
            break;
        case VirtualSpace::Rectangular:
            send(SCI_SETVIRTUALSPACEOPTIONS, SCVS_RECTANGULARSELECTION);
            break;
        case VirtualSpace::Always:
            send(SCI_SETVIRTUALSPACEOPTIONS, SCVS_USERACCESSIBLE);
            break;
    }

    // Scintilla accepts "enable" only while the undo history is empty (the
    // option is meant for documents when they are loaded). Disabling always works.
    int history = SC_CHANGE_HISTORY_DISABLED;
    if (options.changeHistory == ChangeHistory::Margin) {
        history = SC_CHANGE_HISTORY_ENABLED | SC_CHANGE_HISTORY_MARKERS;
    } else if (options.changeHistory == ChangeHistory::Indicator) {
        history = SC_CHANGE_HISTORY_ENABLED | SC_CHANGE_HISTORY_INDICATORS;
    }
    if (history == SC_CHANGE_HISTORY_DISABLED || send(SCI_CANUNDO) == 0) {
        send(SCI_SETCHANGEHISTORY, history);
    }
}

void FernyEditor::zoomIn() {
    send(SCI_ZOOMIN);
}

void FernyEditor::zoomOut() {
    send(SCI_ZOOMOUT);
}

void FernyEditor::resetZoom() {
    send(SCI_SETZOOM, 0);
}

void FernyEditor::applyFont() {
    const QString name = m_fontFamily.isEmpty()
                             ? QFontDatabase::systemFont(QFontDatabase::FixedFont).family()
                             : m_fontFamily;
    const QByteArray family = name.toUtf8();
    sends(SCI_STYLESETFONT, STYLE_DEFAULT, family.constData());
    send(SCI_STYLESETSIZE, STYLE_DEFAULT, m_fontSize);
}

void FernyEditor::setUtf8Text(const QByteArray &utf8) {
    const bool readOnly = send(SCI_GETREADONLY) != 0;
    send(SCI_SETREADONLY, 0);
    send(SCI_CLEARALL);
    // APPENDTEXT takes an explicit length, so embedded NUL bytes are kept.
    send(SCI_APPENDTEXT, static_cast<uptr_t>(utf8.size()),
         reinterpret_cast<sptr_t>(utf8.constData()));
    send(SCI_EMPTYUNDOBUFFER);
    send(SCI_SETSAVEPOINT);
    send(SCI_GOTOPOS, 0);
    send(SCI_SETREADONLY, readOnly ? 1 : 0);
}

void FernyEditor::applyHighlighting(const Highlighting &h) {
    // Default style first: STYLECLEARALL copies it to every style.
    applyFont();
    send(SCI_STYLESETFORE, STYLE_DEFAULT, bgr(h.defaults.fore));
    send(SCI_STYLESETBACK, STYLE_DEFAULT, bgr(h.defaults.back));
    send(SCI_STYLESETBOLD, STYLE_DEFAULT, h.defaults.bold ? 1 : 0);
    send(SCI_STYLESETITALIC, STYLE_DEFAULT, h.defaults.italic ? 1 : 0);
    send(SCI_STYLECLEARALL);

    // Scintilla takes ownership of the lexer and releases it when replaced.
    // Folding is asked of every lexer; the filetype's properties follow.
    Scintilla::ILexer5 *lexer = h.lexer.isEmpty() ? nullptr : CreateLexer(h.lexer.constData());
    send(SCI_SETILEXER, 0, reinterpret_cast<sptr_t>(lexer));
    if (lexer != nullptr) {
        send(SCI_SETPROPERTY, reinterpret_cast<uptr_t>("fold"), reinterpret_cast<sptr_t>("1"));
        for (const auto &[key, value] : h.properties) {
            send(SCI_SETPROPERTY, reinterpret_cast<uptr_t>(key.constData()),
                 reinterpret_cast<sptr_t>(value.constData()));
        }
        for (qsizetype set = 0; set < h.keywords.size(); ++set) {
            sends(SCI_SETKEYWORDS, static_cast<uptr_t>(set), h.keywords.at(set).constData());
        }
        for (const StyleLook &style : h.styles) {
            const auto id = static_cast<uptr_t>(style.style);
            send(SCI_STYLESETFORE, id, bgr(style.fore));
            send(SCI_STYLESETBACK, id, bgr(style.back));
            send(SCI_STYLESETBOLD, id, style.bold ? 1 : 0);
            send(SCI_STYLESETITALIC, id, style.italic ? 1 : 0);
            send(SCI_STYLESETEOLFILLED, id, style.fillEol ? 1 : 0);
        }
    }

    // Editor colours of the scheme (Geany's named styles).
    if (h.lineNumbers) {
        send(SCI_STYLESETFORE, STYLE_LINENUMBER, bgr(h.lineNumbers->fore));
        send(SCI_STYLESETBACK, STYLE_LINENUMBER, bgr(h.lineNumbers->back));
    }
    if (h.caret) {
        send(SCI_SETCARETFORE, static_cast<uptr_t>(bgr(h.caret->fore)));
    }
    if (h.selection) {
        send(SCI_SETSELBACK, 1, bgr(h.selection->back));
    }
    // Geany uses the bold flag of current_line to switch the highlight on.
    send(SCI_SETCARETLINEVISIBLE, h.currentLine && h.currentLine->bold ? 1 : 0);
    if (h.currentLine) {
        send(SCI_SETCARETLINEBACK, static_cast<uptr_t>(bgr(h.currentLine->back)));
    }
    for (const auto &[style, look] :
         {QPair<int, const std::optional<StyleLook> *>{STYLE_BRACELIGHT, &h.braceGood},
          QPair<int, const std::optional<StyleLook> *>{STYLE_BRACEBAD, &h.braceBad}}) {
        if (*look) {
            send(SCI_STYLESETFORE, static_cast<uptr_t>(style), bgr((*look)->fore));
            send(SCI_STYLESETBACK, static_cast<uptr_t>(style), bgr((*look)->back));
            send(SCI_STYLESETBOLD, static_cast<uptr_t>(style), (*look)->bold ? 1 : 0);
        }
    }
    if (h.indentGuide) {
        send(SCI_STYLESETFORE, STYLE_INDENTGUIDE, bgr(h.indentGuide->fore));
    }
    if (h.whiteSpace) {
        send(SCI_SETWHITESPACEFORE, 1, bgr(h.whiteSpace->fore));
    }
    send(SCI_COLOURISE, 0, -1);
}

void FernyEditor::setIndentation(int width, bool useTabs) {
    m_indentWidth = width;
    send(SCI_SETTABWIDTH, static_cast<uptr_t>(width));
    send(SCI_SETINDENT, static_cast<uptr_t>(width));
    send(SCI_SETUSETABS, useTabs ? 1 : 0);
}

void FernyEditor::setControlCharacterLabels(const QStringList &labels) {
    send(SCI_CLEARALLREPRESENTATIONS);
    for (qsizetype i = 0; i + 1 < labels.size(); i += 2) {
        const QByteArray character = labels.at(i).toUtf8();
        const QByteArray label = labels.at(i + 1).toUtf8();
        send(SCI_SETREPRESENTATION, reinterpret_cast<uptr_t>(character.constData()),
             reinterpret_cast<sptr_t>(label.constData()));
        send(SCI_SETREPRESENTATIONAPPEARANCE, reinterpret_cast<uptr_t>(character.constData()),
             SC_REPRESENTATION_BLOB);
    }
}

void FernyEditor::toggleMarker() {
    const sptr_t line = send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETCURRENTPOS)));
    if (send(SCI_MARKERGET, static_cast<uptr_t>(line)) & (1 << kBookmark)) {
        send(SCI_MARKERDELETE, static_cast<uptr_t>(line), kBookmark);
    } else {
        send(SCI_MARKERADD, static_cast<uptr_t>(line), kBookmark);
    }
}

void FernyEditor::gotoMarker(bool forward) {
    const sptr_t line = send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETCURRENTPOS)));
    const sptr_t mask = 1 << kBookmark;
    sptr_t target = forward ? send(SCI_MARKERNEXT, static_cast<uptr_t>(line + 1), mask)
                            : send(SCI_MARKERPREVIOUS, static_cast<uptr_t>(line - 1), mask);
    if (target < 0) {  // wrap around
        target = forward ? send(SCI_MARKERNEXT, 0, mask)
                         : send(SCI_MARKERPREVIOUS, static_cast<uptr_t>(send(SCI_GETLINECOUNT) - 1),
                                mask);
    }
    if (target >= 0) {
        send(SCI_GOTOLINE, static_cast<uptr_t>(target));
    }
}

QString FernyEditor::textRange(sptr_t start, sptr_t end) const {
    QByteArray text(static_cast<qsizetype>(end - start) + 1, '\0');
    Scintilla::TextRangeFull range{{start, end}, text.data()};
    send(SCI_GETTEXTRANGEFULL, 0, reinterpret_cast<sptr_t>(&range));
    text.truncate(static_cast<qsizetype>(end - start));
    return QString::fromUtf8(text);
}

QString FernyEditor::selectedLinesText() const {
    const sptr_t first =
        send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETSELECTIONSTART)));
    const sptr_t last = send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETSELECTIONEND)));
    return textRange(send(SCI_POSITIONFROMLINE, static_cast<uptr_t>(first)),
                     send(SCI_GETLINEENDPOSITION, static_cast<uptr_t>(last)));
}

QString FernyEditor::wordAtCursor(bool beforeCursorOnly) const {
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    const sptr_t start = send(SCI_WORDSTARTPOSITION, static_cast<uptr_t>(pos), 1);
    const sptr_t end =
        beforeCursorOnly ? pos : send(SCI_WORDENDPOSITION, static_cast<uptr_t>(pos), 1);
    return textRange(start, end);
}

void FernyEditor::showCompletions(const QStringList &words, int prefixLength) {
    // One candidate is inserted directly, as Geany does.
    send(SCI_AUTOCSETCHOOSESINGLE, 1);
    send(SCI_AUTOCSETIGNORECASE, 0);
    // Items may hold spaces (a server's completion); none holds a newline.
    send(SCI_AUTOCSETSEPARATOR, '\n');
    const QByteArray list = words.join(QLatin1Char('\n')).toUtf8();
    sends(SCI_AUTOCSHOW, static_cast<uptr_t>(prefixLength), list.constData());
}

void FernyEditor::replaceSelectedLines(const QString &text) {
    const sptr_t first =
        send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETSELECTIONSTART)));
    const sptr_t last = send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETSELECTIONEND)));
    const sptr_t start = send(SCI_POSITIONFROMLINE, static_cast<uptr_t>(first));
    const sptr_t end = send(SCI_GETLINEENDPOSITION, static_cast<uptr_t>(last));
    const QByteArray utf8 = text.toUtf8();
    send(SCI_BEGINUNDOACTION);
    send(SCI_SETTARGETRANGE, static_cast<uptr_t>(start), end);
    send(SCI_REPLACETARGET, static_cast<uptr_t>(utf8.size()),
         reinterpret_cast<sptr_t>(utf8.constData()));
    send(SCI_ENDUNDOACTION);
    // Keep the changed lines selected so the command can be repeated.
    send(SCI_SETSEL, static_cast<uptr_t>(start), start + utf8.size());
}

void FernyEditor::setEditingOptions(const EditingOptions &options) {
    m_autoIndent = options.autoIndent;
    m_newlineStripsTrailing = options.newlineStripsTrailing;
    m_autoCloseQuotes = options.autoCloseQuotes;
    send(SCI_SETTABINDENTS, options.tabIndents);
    send(SCI_SETBACKSPACEUNINDENTS, options.backspaceUnindents);
    // Home and Shift+Home: smart (first non-blank) or plain line start.
    send(SCI_ASSIGNCMDKEY, SCK_HOME, options.smartHome ? SCI_VCHOME : SCI_HOME);
    send(SCI_ASSIGNCMDKEY, SCK_HOME | (SCMOD_SHIFT << 16),
         options.smartHome ? SCI_VCHOMEEXTEND : SCI_HOMEEXTEND);
}

void FernyEditor::onCharAdded(int ch) {
    if (send(SCI_GETSELECTIONS) != 1) {
        return;  // keep multi-caret typing plain
    }
    const sptr_t eolMode = send(SCI_GETEOLMODE);
    if (ch == '\n' || (ch == '\r' && eolMode == SC_EOL_CR)) {
        if (m_autoIndent) {
            autoIndent();
        }
        // After the indentation is set, so the new line still copies the
        // previous line's indentation.
        if (m_newlineStripsTrailing) {
            stripTrailingSpaces(
                send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETCURRENTPOS))) - 1);
        }
        return;
    }
    if ((ch == '"' || ch == '\'') && m_autoCloseQuotes) {
        autoCloseQuote(ch);
        return;
    }
    if (!m_autoClose) {
        return;
    }
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    const int next = static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos)));
    if (isOpener(ch)) {
        // Only before whitespace, the end of a line or a closing bracket.
        if (next == 0 || next == ' ' || next == '\t' || next == '\r' || next == '\n' ||
            next == ')' || next == ']' || next == '}') {
            const char closer[2] = {closerFor(ch), '\0'};
            send(SCI_INSERTTEXT, static_cast<uptr_t>(pos), reinterpret_cast<sptr_t>(closer));
        }
    } else if ((ch == ')' || ch == ']' || ch == '}') && next == ch) {
        // Type over the closer that auto-close inserted.
        send(SCI_DELETERANGE, static_cast<uptr_t>(pos), 1);
    }
}

void FernyEditor::stripTrailingSpaces(sptr_t line) {
    if (line < 0) {
        return;
    }
    const sptr_t start = send(SCI_POSITIONFROMLINE, static_cast<uptr_t>(line));
    const sptr_t end = send(SCI_GETLINEENDPOSITION, static_cast<uptr_t>(line));
    sptr_t scan = end;
    while (scan > start) {
        const int c = static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(scan - 1)));
        if (c != ' ' && c != '\t') {
            break;
        }
        --scan;
    }
    if (scan < end) {
        send(SCI_DELETERANGE, static_cast<uptr_t>(scan), end - scan);
    }
}

void FernyEditor::autoCloseQuote(int quote) {
    // The typed quote is already in the text, just before the caret.
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    const int next = static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos)));
    const int before =
        pos >= 2 ? static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos - 2))) : 0;
    if (next == quote) {
        // Type over the closing quote that is already there.
        send(SCI_DELETERANGE, static_cast<uptr_t>(pos - 1), 1);
        send(SCI_GOTOPOS, static_cast<uptr_t>(pos));
        return;
    }
    // Not after a word (an apostrophe in "don't") and only before a space,
    // a line end or a closing bracket.
    const bool wordBefore =
        (before > 0 && std::isalnum(before)) || before == '_' || before == quote;
    const bool closesHere = next == 0 || next == ' ' || next == '\t' || next == '\r' ||
                            next == '\n' || next == ')' || next == ']' || next == '}';
    if (!wordBefore && closesHere) {
        const char closer[2] = {static_cast<char>(quote), '\0'};
        send(SCI_INSERTTEXT, static_cast<uptr_t>(pos), reinterpret_cast<sptr_t>(closer));
    }
}

void FernyEditor::autoIndent() {
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    const sptr_t line = send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(pos));
    if (line == 0) {
        return;
    }
    const sptr_t previous = line - 1;
    sptr_t indent = send(SCI_GETLINEINDENTATION, static_cast<uptr_t>(previous));
    // One level more after an opening bracket at the end of the line.
    const sptr_t prevEnd = send(SCI_GETLINEENDPOSITION, static_cast<uptr_t>(previous));
    sptr_t scan = prevEnd;
    const sptr_t prevStart = send(SCI_POSITIONFROMLINE, static_cast<uptr_t>(previous));
    while (scan > prevStart) {
        const int c = static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(scan - 1)));
        if (c != ' ' && c != '\t') {
            break;
        }
        --scan;
    }
    const bool opened =
        scan > prevStart &&
        isOpener(static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(scan - 1))));
    const int next = static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos)));
    if (opened && m_autoClose && (next == ')' || next == ']' || next == '}')) {
        // Enter between an auto-closed pair: the closer goes to its own line.
        const sptr_t mode = send(SCI_GETEOLMODE);
        const QByteArray eol = mode == SC_EOL_CRLF ? QByteArrayLiteral("\r\n")
                               : mode == SC_EOL_CR ? QByteArrayLiteral("\r")
                                                   : QByteArrayLiteral("\n");
        send(SCI_INSERTTEXT, static_cast<uptr_t>(pos), reinterpret_cast<sptr_t>(eol.constData()));
        send(SCI_SETLINEINDENTATION, static_cast<uptr_t>(line + 1), indent);
    }
    if (opened) {
        indent += m_indentWidth;
    }
    send(SCI_SETLINEINDENTATION, static_cast<uptr_t>(line), indent);
    send(SCI_GOTOPOS,
         static_cast<uptr_t>(send(SCI_GETLINEINDENTPOSITION, static_cast<uptr_t>(line))));
}

void FernyEditor::highlightBraces() {
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    auto isBrace = [](int c) {
        return c == '(' || c == ')' || c == '[' || c == ']' || c == '{' || c == '}';
    };
    sptr_t brace = -1;
    if (pos > 0 && isBrace(static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos - 1))))) {
        brace = pos - 1;
    } else if (isBrace(static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos))))) {
        brace = pos;
    }
    if (brace < 0) {
        send(SCI_BRACEHIGHLIGHT, static_cast<uptr_t>(INVALID_POSITION), INVALID_POSITION);
        return;
    }
    const sptr_t match = send(SCI_BRACEMATCH, static_cast<uptr_t>(brace), 0);
    if (match < 0) {
        send(SCI_BRACEBADLIGHT, static_cast<uptr_t>(brace));
    } else {
        send(SCI_BRACEHIGHLIGHT, static_cast<uptr_t>(brace), match);
    }
}

void FernyEditor::replaceBeforeCursor(int before, const QString &text, int cursor) {
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    const sptr_t start = pos - before;
    const QByteArray utf8 = text.toUtf8();
    send(SCI_BEGINUNDOACTION);
    send(SCI_SETTARGETRANGE, static_cast<uptr_t>(start), pos);
    send(SCI_REPLACETARGET, static_cast<uptr_t>(utf8.size()),
         reinterpret_cast<sptr_t>(utf8.constData()));
    send(SCI_ENDUNDOACTION);
    const qsizetype offset = cursor >= 0 ? text.left(cursor).toUtf8().size() : utf8.size();
    send(SCI_GOTOPOS, static_cast<uptr_t>(start + offset));
}

void FernyEditor::replaceAllText(const QByteArray &utf8) {
    const quint32 line = cursorLine();
    send(SCI_BEGINUNDOACTION);
    send(SCI_TARGETWHOLEDOCUMENT);
    send(SCI_REPLACETARGET, static_cast<uptr_t>(utf8.size()),
         reinterpret_cast<sptr_t>(utf8.constData()));
    send(SCI_ENDUNDOACTION);
    gotoPosition(line, 0);
}

QString FernyEditor::lineIndentation() const {
    const sptr_t line = send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(send(SCI_GETCURRENTPOS)));
    const sptr_t start = send(SCI_POSITIONFROMLINE, static_cast<uptr_t>(line));
    const sptr_t end = send(SCI_GETLINEINDENTPOSITION, static_cast<uptr_t>(line));
    return textRange(start, end);
}

int FernyEditor::markOccurrences() {
    clearMarks();
    const QString selected = selectedText();
    const QString word = !selected.isEmpty() && !selected.contains(QLatin1Char('\n'))
                             ? selected
                             : wordAtCursor(false);
    if (word.isEmpty()) {
        return 0;
    }
    const QByteArray utf8 = word.toUtf8();
    const sptr_t length = send(SCI_GETLENGTH);
    const sptr_t flags = send(SCI_GETSEARCHFLAGS);
    send(SCI_SETSEARCHFLAGS, SCFIND_MATCHCASE | SCFIND_WHOLEWORD);
    send(SCI_INDICSETSTYLE, kMarkIndicator, INDIC_ROUNDBOX);
    send(SCI_INDICSETFORE, kMarkIndicator, 0x00A0FF);  // BGR: orange
    send(SCI_SETINDICATORCURRENT, kMarkIndicator);
    int count = 0;
    sptr_t pos = 0;
    while (pos < length) {
        send(SCI_SETTARGETRANGE, static_cast<uptr_t>(pos), length);
        const sptr_t found = send(SCI_SEARCHINTARGET, static_cast<uptr_t>(utf8.size()),
                                  reinterpret_cast<sptr_t>(utf8.constData()));
        if (found < 0) {
            break;
        }
        const sptr_t end = send(SCI_GETTARGETEND);
        send(SCI_INDICATORFILLRANGE, static_cast<uptr_t>(found), end - found);
        ++count;
        pos = std::max(end, found + 1);
    }
    send(SCI_SETSEARCHFLAGS, static_cast<uptr_t>(flags));
    return count;
}

void FernyEditor::clearMarks() {
    send(SCI_SETINDICATORCURRENT, kMarkIndicator);
    send(SCI_INDICATORCLEARRANGE, 0, send(SCI_GETLENGTH));
}

void FernyEditor::keyPressEvent(QKeyEvent *event) {
    // Enclose (Geany's addons, H4.2a): an opener or a quote typed over one
    // selection wraps it instead of replacing it.
    if (m_autoClose && event->text().size() == 1 && send(SCI_GETSELECTIONS) == 1 &&
        !send(SCI_GETSELECTIONEMPTY)) {
        const char open = event->text().at(0).toLatin1();
        if (isOpener(open) || open == '"' || open == '\'' || open == '`') {
            const char pair[3] = {open, isOpener(open) ? closerFor(open) : open, '\0'};
            const sptr_t start = send(SCI_GETSELECTIONSTART);
            const sptr_t end = send(SCI_GETSELECTIONEND);
            send(SCI_BEGINUNDOACTION);
            send(SCI_INSERTTEXT, static_cast<uptr_t>(end), reinterpret_cast<sptr_t>(pair + 1));
            const char opener[2] = {open, '\0'};
            send(SCI_INSERTTEXT, static_cast<uptr_t>(start), reinterpret_cast<sptr_t>(opener));
            send(SCI_ENDUNDOACTION);
            send(SCI_SETSEL, static_cast<uptr_t>(start + 1), end + 1);
            event->accept();
            return;
        }
    }
    if (event->key() == Qt::Key_Tab && event->modifiers() == Qt::NoModifier &&
        send(SCI_GETSELECTIONS) == 1 && send(SCI_GETSELECTIONEMPTY) && m_tabHandler &&
        m_tabHandler()) {
        event->accept();
        return;
    }
    // Backspace inside an empty auto-closed pair deletes both characters.
    if (m_autoClose && event->key() == Qt::Key_Backspace && event->modifiers() == Qt::NoModifier &&
        send(SCI_GETSELECTIONS) == 1 && send(SCI_GETSELECTIONEMPTY)) {
        const sptr_t pos = send(SCI_GETCURRENTPOS);
        if (pos > 0) {
            const int before = static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos - 1)));
            const int after = static_cast<int>(send(SCI_GETCHARAT, static_cast<uptr_t>(pos)));
            if (isOpener(before) && after == closerFor(before)) {
                send(SCI_DELETERANGE, static_cast<uptr_t>(pos - 1), 2);
                event->accept();
                return;
            }
        }
    }
    ScintillaEditBase::keyPressEvent(event);
}

quint32 FernyEditor::defaultBackground() const {
    return rgb(send(SCI_STYLEGETBACK, STYLE_DEFAULT));
}

QString FernyEditor::lexerName() const {
    const sptr_t length = send(SCI_GETLEXERLANGUAGE, 0, 0);
    if (length <= 0) {
        return {};
    }
    QByteArray name(static_cast<qsizetype>(length) + 1, '\0');
    send(SCI_GETLEXERLANGUAGE, 0, reinterpret_cast<sptr_t>(name.data()));
    name.truncate(static_cast<qsizetype>(length));
    return QString::fromUtf8(name);
}

QString FernyEditor::selectedText() const {
    const sptr_t start = send(SCI_GETSELECTIONSTART);
    const sptr_t end = send(SCI_GETSELECTIONEND);
    // GETTEXTRANGEFULL writes a terminating NUL: allocate one extra byte.
    QByteArray text(static_cast<qsizetype>(end - start) + 1, '\0');
    Sci_TextRangeFull range{{start, end}, text.data()};
    send(SCI_GETTEXTRANGEFULL, 0, reinterpret_cast<sptr_t>(&range));
    text.chop(1);
    return QString::fromUtf8(text);
}

void FernyEditor::applyPluginEdits(const QStringList &edits) {
    send(SCI_BEGINUNDOACTION);
    for (qsizetype i = 0; i + 1 < edits.size(); i += 2) {
        const QByteArray utf8 = edits.at(i + 1).toUtf8();
        if (edits.at(i) == QStringLiteral("insert")) {
            const sptr_t pos = send(SCI_GETCURRENTPOS);
            send(SCI_INSERTTEXT, static_cast<uptr_t>(pos),
                 reinterpret_cast<sptr_t>(utf8.constData()));
            send(SCI_GOTOPOS, static_cast<uptr_t>(pos + utf8.size()));
        } else {
            send(SCI_REPLACESEL, 0, reinterpret_cast<sptr_t>(utf8.constData()));
        }
    }
    send(SCI_ENDUNDOACTION);
}

QByteArray FernyEditor::utf8Text() const {
    const qint64 size = length();
    // GETTEXT writes a terminating NUL: allocate one extra byte, then drop it.
    QByteArray text(static_cast<qsizetype>(size) + 1, '\0');
    send(SCI_GETTEXT, static_cast<uptr_t>(size) + 1, reinterpret_cast<sptr_t>(text.data()));
    text.truncate(static_cast<qsizetype>(size));
    return text;
}

bool FernyEditor::isModified() const {
    return send(SCI_GETMODIFY) != 0;
}

void FernyEditor::setSavePoint() {
    send(SCI_SETSAVEPOINT);
}

void FernyEditor::setReadOnly(bool readOnly) {
    send(SCI_SETREADONLY, readOnly ? 1 : 0);
}

void FernyEditor::setEolMode(int mode) {
    send(SCI_SETEOLMODE, static_cast<uptr_t>(mode));
}

qint64 FernyEditor::length() const {
    return send(SCI_GETLENGTH);
}

void FernyEditor::gotoPosition(quint32 line, quint32 column) {
    const sptr_t lines = send(SCI_GETLINECOUNT);
    const sptr_t targetLine = std::clamp<sptr_t>(line > 0 ? line - 1 : 0, 0, lines - 1);
    const sptr_t targetColumn = column > 0 ? column - 1 : 0;
    // FINDCOLUMN stops at the end of the line, so long columns are clamped.
    const sptr_t pos = send(SCI_FINDCOLUMN, static_cast<uptr_t>(targetLine), targetColumn);
    send(SCI_GOTOPOS, static_cast<uptr_t>(pos));
    send(SCI_SCROLLCARET);
}

quint32 FernyEditor::cursorLine() const {
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    return static_cast<quint32>(send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(pos)) + 1);
}

quint32 FernyEditor::cursorColumn() const {
    const sptr_t pos = send(SCI_GETCURRENTPOS);
    return static_cast<quint32>(send(SCI_GETCOLUMN, static_cast<uptr_t>(pos)) + 1);
}

qint64 FernyEditor::lineCount() const {
    return static_cast<qint64>(send(SCI_GETLINECOUNT));
}

qint64 FernyEditor::selectedCharacters() const {
    qint64 total = 0;
    const sptr_t selections = send(SCI_GETSELECTIONS);
    for (sptr_t i = 0; i < selections; ++i) {
        const sptr_t start = send(SCI_GETSELECTIONNSTART, static_cast<uptr_t>(i));
        const sptr_t end = send(SCI_GETSELECTIONNEND, static_cast<uptr_t>(i));
        total += static_cast<qint64>(
            send(SCI_COUNTCHARACTERS, static_cast<uptr_t>(start), static_cast<sptr_t>(end)));
    }
    return total;
}

int FernyEditor::indentWidth() const {
    return static_cast<int>(send(SCI_GETINDENT));
}

bool FernyEditor::indentUsesTabs() const {
    return send(SCI_GETUSETABS) != 0;
}

bool FernyEditor::hasStyledText(qint64 maxBytes) const {
    const qint64 end = std::min(length(), maxBytes);
    send(SCI_COLOURISE, 0, static_cast<sptr_t>(end));
    for (qint64 pos = 0; pos < end; ++pos) {
        const sptr_t style = send(SCI_GETSTYLEAT, static_cast<uptr_t>(pos));
        if (style != 0 && style != STYLE_DEFAULT) {
            return true;
        }
    }
    return false;
}

}  // namespace ferny
