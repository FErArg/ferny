// The message window: Status (status-bar history), Messages (application
// messages), Find in Files results and Scribble (free notes saved in the
// session). Nothing shown here is written to logs (threat T10).
#pragma once

#include <QString>
#include <QTabWidget>

class QLabel;
class QListWidget;
class QPlainTextEdit;
class QTreeWidget;

namespace ferny {

class MessageWindow : public QTabWidget {
    Q_OBJECT

public:
    // `scribbleLimit`: largest Scribble saved, in UTF-8 bytes.
    MessageWindow(QTreeWidget *results, int scribbleLimit, QWidget *parent = nullptr);

    // Appends a time-stamped line to Status (empty text is ignored).
    void appendStatus(const QString &text);
    // Appends a time-stamped line to Messages and shows that tab.
    void appendMessage(const QString &text);

    // Compiler output (H2.4): one item per line, filled by MainWindow.
    QListWidget *compiler() const { return m_compiler; }
    void showCompiler() { setCurrentIndex(m_compilerIndex); }

    // Shows the Find in Files tab with `summary` above the results.
    void showResults(const QString &summary);

    QString scribble() const;
    void setScribble(const QString &text);

    // Lines kept in Status and Messages.
    static constexpr int kMaxLines = 1000;

private:
    QPlainTextEdit *m_status;
    QPlainTextEdit *m_messages;
    QListWidget *m_compiler;
    int m_compilerIndex = 0;
    QLabel *m_resultsSummary;
    QPlainTextEdit *m_scribble;
    int m_resultsIndex = 0;
    int m_messagesIndex = 0;
    int m_scribbleLimit;
    bool m_scribbleWarned = false;
};

}  // namespace ferny
