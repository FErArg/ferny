#include "findbar.h"

#include <QCheckBox>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QVBoxLayout>

namespace ferny {

FindBar::FindBar(QWidget *parent)
    : QWidget(parent),
      m_pattern(new QLineEdit(this)),
      m_replacement(new QLineEdit(this)),
      m_replaceRow(new QWidget(this)),
      m_matchCase(new QCheckBox(tr("Match &case"), this)),
      m_wholeWord(new QCheckBox(tr("&Whole word"), this)),
      m_regex(new QCheckBox(tr("Regular e&xpression"), this)),
      m_message(new QLabel(this)) {
    m_pattern->setPlaceholderText(tr("Find"));
    m_replacement->setPlaceholderText(tr("Replace with"));
    m_pattern->setClearButtonEnabled(true);

    auto *previous = new QPushButton(tr("&Previous"), this);
    auto *next = new QPushButton(tr("&Next"), this);
    auto *close = new QPushButton(tr("Close"), this);
    auto *replace = new QPushButton(tr("&Replace"), m_replaceRow);
    auto *replaceAll = new QPushButton(tr("Replace &All"), m_replaceRow);
    for (QPushButton *button : {previous, next, close, replace, replaceAll}) {
        button->setAutoDefault(false);
    }

    auto *findRow = new QHBoxLayout;
    findRow->addWidget(m_pattern, 1);
    findRow->addWidget(previous);
    findRow->addWidget(next);
    findRow->addWidget(m_matchCase);
    findRow->addWidget(m_wholeWord);
    findRow->addWidget(m_regex);
    findRow->addWidget(m_message);
    findRow->addWidget(close);

    auto *replaceLayout = new QHBoxLayout(m_replaceRow);
    replaceLayout->setContentsMargins(0, 0, 0, 0);
    replaceLayout->addWidget(m_replacement, 1);
    replaceLayout->addWidget(replace);
    replaceLayout->addWidget(replaceAll);
    replaceLayout->addStretch(1);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(6, 4, 6, 4);
    layout->addLayout(findRow);
    layout->addWidget(m_replaceRow);

    connect(next, &QPushButton::clicked, this, &FindBar::findNext);
    connect(previous, &QPushButton::clicked, this, &FindBar::findPrevious);
    connect(replace, &QPushButton::clicked, this, &FindBar::replaceOne);
    connect(replaceAll, &QPushButton::clicked, this, &FindBar::replaceAll);
    connect(close, &QPushButton::clicked, this, [this]() {
        hide();
        Q_EMIT closed();
    });
    connect(m_pattern, &QLineEdit::textChanged, this, [this]() { setMessage(QString()); });
    hide();
}

void FindBar::open(bool replace, const QString &seed) {
    if (!seed.isEmpty() && !seed.contains(QLatin1Char('\n'))) {
        m_pattern->setText(seed);
    }
    m_replaceRow->setVisible(replace);
    show();
    m_pattern->setFocus();
    m_pattern->selectAll();
}

QString FindBar::pattern() const {
    return m_pattern->text();
}

QString FindBar::replacement() const {
    return m_replacement->text();
}

quint32 FindBar::flags() const {
    return (m_regex->isChecked() ? 1u : 0u) | (m_matchCase->isChecked() ? 2u : 0u) |
           (m_wholeWord->isChecked() ? 4u : 0u);
}

void FindBar::setPattern(const QString &pattern) {
    m_pattern->setText(pattern);
}

void FindBar::setReplacement(const QString &replacement) {
    m_replacement->setText(replacement);
}

void FindBar::setFlags(quint32 flags) {
    m_regex->setChecked((flags & 1u) != 0);
    m_matchCase->setChecked((flags & 2u) != 0);
    m_wholeWord->setChecked((flags & 4u) != 0);
}

void FindBar::setMessage(const QString &message) {
    m_message->setText(message);
}

void FindBar::keyPressEvent(QKeyEvent *event) {
    switch (event->key()) {
        case Qt::Key_Escape:
            hide();
            Q_EMIT closed();
            return;
        case Qt::Key_Return:
        case Qt::Key_Enter:
            if (m_replacement->hasFocus()) {
                Q_EMIT replaceOne();
            } else if (event->modifiers() & Qt::ShiftModifier) {
                Q_EMIT findPrevious();
            } else {
                Q_EMIT findNext();
            }
            return;
        default:
            QWidget::keyPressEvent(event);
    }
}

}  // namespace ferny
