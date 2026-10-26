#include "messagewindow.h"

#include <QLabel>
#include <QListWidget>
#include <QPlainTextEdit>
#include <QTime>
#include <QTreeWidget>
#include <QVBoxLayout>

namespace ferny {

namespace {

QPlainTextEdit *logView(QWidget *parent) {
    auto *view = new QPlainTextEdit(parent);
    view->setReadOnly(true);
    view->setMaximumBlockCount(MessageWindow::kMaxLines);
    view->setLineWrapMode(QPlainTextEdit::NoWrap);
    return view;
}

QString stamped(const QString &text) {
    // One line per entry: line breaks in a message would forge entries.
    QString line = text;
    line.replace(QLatin1Char('\n'), QLatin1Char(' ')).replace(QLatin1Char('\r'), QLatin1Char(' '));
    return QTime::currentTime().toString(QStringLiteral("HH:mm:ss")) + QStringLiteral(": ") + line;
}

}  // namespace

MessageWindow::MessageWindow(QTreeWidget *results, int scribbleLimit, QWidget *parent)
    : QTabWidget(parent),
      m_status(logView(this)),
      m_messages(logView(this)),
      m_compiler(new QListWidget(this)),
      m_resultsSummary(new QLabel(this)),
      m_scribble(new QPlainTextEdit(this)),
      m_scribbleLimit(scribbleLimit) {
    setDocumentMode(true);
    setTabPosition(QTabWidget::South);
    addTab(m_status, tr("Status"));
    m_compiler->setUniformItemSizes(true);
    m_compilerIndex = addTab(m_compiler, tr("Compiler"));
    m_messagesIndex = addTab(m_messages, tr("Messages"));

    auto *resultsPage = new QWidget(this);
    auto *layout = new QVBoxLayout(resultsPage);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(2);
    m_resultsSummary->setTextFormat(Qt::PlainText);
    layout->addWidget(m_resultsSummary);
    results->setParent(resultsPage);
    layout->addWidget(results, 1);
    m_resultsIndex = addTab(resultsPage, tr("Find in Files"));

    m_scribble->setPlaceholderText(tr("Notes kept between sessions"));
    addTab(m_scribble, tr("Scribble"));
    connect(m_scribble, &QPlainTextEdit::textChanged, this, [this]() {
        const bool over = m_scribble->toPlainText().toUtf8().size() > m_scribbleLimit;
        if (over && !m_scribbleWarned) {
            appendStatus(tr("The Scribble is larger than %1 MiB; the extra text will not be saved.")
                             .arg(m_scribbleLimit / (1024 * 1024)));
        }
        m_scribbleWarned = over;
    });
}

void MessageWindow::appendStatus(const QString &text) {
    if (!text.isEmpty()) {
        m_status->appendPlainText(stamped(text));
    }
}

void MessageWindow::appendMessage(const QString &text) {
    if (!text.isEmpty()) {
        m_messages->appendPlainText(stamped(text));
        setCurrentIndex(m_messagesIndex);
    }
}

void MessageWindow::showResults(const QString &summary) {
    m_resultsSummary->setText(summary);
    setCurrentIndex(m_resultsIndex);
}

QString MessageWindow::scribble() const {
    return m_scribble->toPlainText();
}

void MessageWindow::setScribble(const QString &text) {
    m_scribble->setPlainText(text);
}

}  // namespace ferny
