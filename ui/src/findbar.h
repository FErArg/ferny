// Find/replace bar shown under the editor tabs. It only collects the query;
// MainWindow runs the search through the core (linear-time regex engine).
#pragma once

#include <QString>
#include <QWidget>

class QCheckBox;
class QLabel;
class QLineEdit;
class QPushButton;

namespace ferny {

class FindBar : public QWidget {
    Q_OBJECT

public:
    explicit FindBar(QWidget *parent = nullptr);

    // Shows the bar (with the replace row if `replace`), seeding the
    // pattern with `seed` when not empty, and focuses the pattern field.
    void open(bool replace, const QString &seed);

    QString pattern() const;
    QString replacement() const;
    // Core search flags: 1 regex, 2 match case, 4 whole word.
    quint32 flags() const;

    void setPattern(const QString &pattern);
    void setReplacement(const QString &replacement);
    void setFlags(quint32 flags);
    // Short feedback next to the fields ("Not found", "Invalid pattern").
    void setMessage(const QString &message);

Q_SIGNALS:
    void findNext();
    void findPrevious();
    void replaceOne();
    void replaceAll();
    void closed();

protected:
    void keyPressEvent(QKeyEvent *event) override;

private:
    QLineEdit *m_pattern;
    QLineEdit *m_replacement;
    QWidget *m_replaceRow;
    QCheckBox *m_matchCase;
    QCheckBox *m_wholeWord;
    QCheckBox *m_regex;
    QLabel *m_message;
};

}  // namespace ferny
