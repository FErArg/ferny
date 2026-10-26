#include "preferences.h"

#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialogButtonBox>
#include <QFontComboBox>
#include <QFontDatabase>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QPushButton>
#include <QScrollArea>
#include <QSpinBox>
#include <QStackedWidget>
#include <QTableWidget>
#include <QVBoxLayout>

namespace ferny {

namespace {

// Titles are English source strings from the core.
QString translated(const char *context, const QString &source) {
    const QByteArray utf8 = source.toUtf8();
    return QCoreApplication::translate(context, utf8.constData());
}

QString sectionTitle(const QString &section) {
    if (section == QStringLiteral("editor")) {
        return QCoreApplication::translate("ferny::settings", "Editor");
    }
    if (section == QStringLiteral("files")) {
        return QCoreApplication::translate("ferny::settings", "Files");
    }
    if (section == QStringLiteral("templates")) {
        return QCoreApplication::translate("ferny::settings", "Templates");
    }
    if (section == QStringLiteral("ui")) {
        return QCoreApplication::translate("ferny::settings", "Interface");
    }
    if (section == QStringLiteral("spell")) {
        return QCoreApplication::translate("ferny::settings", "Spell Checking");
    }
    return section;
}

}  // namespace

PreferencesDialog::PreferencesDialog(FernyCore &core, QWidget *parent)
    : QDialog(parent), m_core(core) {
    setWindowTitle(tr("Preferences"));
    resize(760, 540);

    // Category list (left) + stacked pages (right). Every descriptor lands
    // exactly once on a populated page; Trusted Folders is the last
    // category and keeps its immediate revocation (H2.1).
    auto *layout = new QVBoxLayout(this);
    auto *body = new QHBoxLayout;
    auto *categories = new QListWidget(this);
    categories->setFixedWidth(180);
    auto *stack = new QStackedWidget(this);
    auto *scroll = new QScrollArea(this);
    scroll->setWidget(stack);
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    body->addWidget(categories);
    body->addWidget(scroll, 1);
    layout->addLayout(body, 1);

    const QStringList keys = m_core.settingKeys();

    // Assign every key to a page. General gathers the interface, spelling
    // and hidden-files options plus the shortcuts link; unknown future
    // sections keep their own page so no descriptor is ever dropped.
    QHash<QString, QString> keyPage;
    QStringList pageOrder{QStringLiteral("general"), QStringLiteral("editor"),
                          QStringLiteral("files"), QStringLiteral("templates")};
    for (const QString &key : keys) {
        const QString section = key.section(QLatin1Char('.'), 0, 0);
        const QString page =
            (section == QStringLiteral("ui") || section == QStringLiteral("spell") ||
             key == QStringLiteral("files.browser_show_hidden"))
                ? QStringLiteral("general")
                : section;
        keyPage.insert(key, page);
        if (!pageOrder.contains(page)) {
            pageOrder.append(page);
        }
    }
    QStringList usedPages;
    for (const QString &page : std::as_const(pageOrder)) {
        for (const QString &key : std::as_const(keys)) {
            if (keyPage.value(key) == page) {
                usedPages.append(page);
                break;
            }
        }
    }

    QHash<QString, QFormLayout *> forms;
    for (const QString &page : std::as_const(usedPages)) {
        auto *pageWidget = new QWidget(stack);
        forms.insert(page, new QFormLayout(pageWidget));
        stack->addWidget(pageWidget);
        const QString title = page == QStringLiteral("general")
                                  ? QCoreApplication::translate("ferny::settings", "General")
                              : page == QStringLiteral("security") ? tr("Security")
                                                                   : sectionTitle(page);
        categories->addItem(title);
        m_pageTitles.append(title);
    }

    // Help text only where the behaviour is confirmed (M3.2): the language
    // change needs a restart, and .editorconfig/filetypes may override
    // editor options per document.
    if (QFormLayout *generalForm = forms.value(QStringLiteral("general"))) {
        auto *restart =
            new QLabel(tr("Changing the interface language requires restarting Ferny."), this);
        restart->setWordWrap(true);
        generalForm->addRow(restart);
        auto *shortcuts = new QPushButton(tr("&Keyboard Shortcuts…"), this);
        generalForm->addRow(QString(), shortcuts);
        connect(shortcuts, &QPushButton::clicked, this, &PreferencesDialog::editShortcutsRequested);
    }
    if (QFormLayout *editorForm = forms.value(QStringLiteral("editor"))) {
        auto *note = new QLabel(
            tr("Some options can be overridden per document by .editorconfig and filetype "
               "settings."),
            this);
        note->setWordWrap(true);
        editorForm->addRow(note);
    }

    for (const QString &key : keys) {
        const QString kind = m_core.settingKind(key);
        const QString value = m_core.settingValue(key);
        const QStringList options = m_core.settingOptions(key);
        QWidget *editor = nullptr;
        if (kind == QStringLiteral("bool")) {
            auto *box = new QCheckBox(this);
            box->setChecked(value == QStringLiteral("true"));
            editor = box;
        } else if (kind == QStringLiteral("int") && options.size() == 2) {
            auto *spin = new QSpinBox(this);
            spin->setRange(options.at(0).toInt(), options.at(1).toInt());
            spin->setValue(value.toInt());
            editor = spin;
        } else if (kind == QStringLiteral("text")) {
            editor = new QLineEdit(value, this);
        } else if (kind == QStringLiteral("font")) {
            auto *font = new QFontComboBox(this);
            font->setFontFilters(QFontComboBox::MonospacedFonts);
            font->setCurrentFont(value.isEmpty()
                                     ? QFontDatabase::systemFont(QFontDatabase::FixedFont)
                                     : QFont(value));
            editor = font;
        } else {
            auto *combo = new QComboBox(this);
            QStringList items = options;
            if (kind == QStringLiteral("encoding")) {
                items = m_core.supportedEncodings();
            }
            if (kind == QStringLiteral("scheme")) {
                for (const QString &id : m_core.colorSchemeIds()) {
                    combo->addItem(m_core.colorSchemeName(id), id);
                }
            } else {
                for (const QString &item : items) {
                    // Choice values are English source strings from the
                    // core; encodings are names and stay as they are.
                    combo->addItem(kind == QStringLiteral("choice")
                                       ? translated("ferny::settings", item)
                                       : item,
                                   item);
                }
            }
            combo->setCurrentIndex(std::max(0, combo->findData(value)));
            editor = combo;
        }
        m_editors.insert(key, editor);
        QFormLayout *form = forms.value(keyPage.value(key));
        Q_ASSERT(form != nullptr);
        form->addRow(translated("ferny::settings", m_core.settingTitle(key)), editor);
    }

    // Security (H2.1): trusted folders, revoked at once (not on OK).
    auto *securityPage = new QWidget(stack);
    auto *securityLayout = new QVBoxLayout(securityPage);
    auto *security = new QGroupBox(tr("Trusted Folders"), securityPage);
    auto *trusted = new QListWidget(security);
    trusted->addItems(m_core.trustedFolders());
    auto *revoke = new QPushButton(tr("&Revoke"), security);
    auto *groupLayout = new QVBoxLayout(security);
    groupLayout->addWidget(trusted);
    groupLayout->addWidget(revoke, 0, Qt::AlignLeft);
    auto *immediate = new QLabel(
        tr("Revoking a folder takes effect immediately, even if you cancel."), securityPage);
    immediate->setWordWrap(true);
    securityLayout->addWidget(security);
    securityLayout->addWidget(immediate);
    stack->addWidget(securityPage);
    categories->addItem(tr("Security"));
    m_pageTitles.append(tr("Security"));
    connect(revoke, &QPushButton::clicked, this, [this, trusted]() {
        if (QListWidgetItem *item = trusted->currentItem();
            item && m_core.revokeFolder(item->text())) {
            delete item;
        }
    });

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(categories, &QListWidget::currentRowChanged, stack, &QStackedWidget::setCurrentIndex);
    categories->setCurrentRow(0);
}

QStringList PreferencesDialog::editedKeys() const {
    return m_editors.keys();
}

QStringList PreferencesDialog::pageTitles() const {
    return m_pageTitles;
}

QStringList PreferencesDialog::apply() {
    QStringList rejected;
    for (auto it = m_editors.cbegin(); it != m_editors.cend(); ++it) {
        QString value;
        if (auto *box = qobject_cast<QCheckBox *>(it.value())) {
            value = box->isChecked() ? QStringLiteral("true") : QStringLiteral("false");
        } else if (auto *spin = qobject_cast<QSpinBox *>(it.value())) {
            value = QString::number(spin->value());
        } else if (auto *font = qobject_cast<QFontComboBox *>(it.value())) {
            value = font->currentFont().family();
        } else if (auto *line = qobject_cast<QLineEdit *>(it.value())) {
            value = line->text().trimmed();
        } else if (auto *combo = qobject_cast<QComboBox *>(it.value())) {
            value = combo->currentData().toString();
        }
        if (!m_core.setSetting(it.key(), value)) {
            rejected.append(it.key());
        }
    }
    return rejected;
}

ShortcutsDialog::ShortcutsDialog(FernyCore &core, QWidget *parent)
    : QDialog(parent), m_core(core), m_table(new QTableWidget(this)), m_ids(core.commandIds()) {
    setWindowTitle(tr("Keyboard Shortcuts"));
    resize(640, 520);
    m_table->setColumnCount(3);
    m_table->setHorizontalHeaderLabels({tr("Command"), tr("Id"), tr("Shortcut")});
    m_table->setRowCount(static_cast<int>(m_ids.size()));
    m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_table->verticalHeader()->hide();
    m_table->horizontalHeader()->setStretchLastSection(true);
    for (int row = 0; row < m_ids.size(); ++row) {
        QString title = translated("ferny::commands", m_core.commandTitle(m_ids.at(row)));
        title.remove(QLatin1Char('&'));
        m_table->setItem(row, 0, new QTableWidgetItem(title));
        m_table->setItem(row, 1, new QTableWidgetItem(m_ids.at(row)));
        refreshRow(row);
    }
    m_table->resizeColumnsToContents();

    auto *change = new QPushButton(tr("&Change…"), this);
    auto *clear = new QPushButton(tr("C&lear"), this);
    auto *reset = new QPushButton(tr("&Reset to Default"), this);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    auto *row = new QHBoxLayout;
    row->addWidget(change);
    row->addWidget(clear);
    row->addWidget(reset);
    row->addStretch(1);
    auto *layout = new QVBoxLayout(this);
    layout->addWidget(m_table, 1);
    layout->addLayout(row);
    layout->addWidget(buttons);

    connect(m_table, &QTableWidget::cellDoubleClicked, this, [this](int r, int) { editRow(r); });
    connect(change, &QPushButton::clicked, this, [this]() { editRow(m_table->currentRow()); });
    connect(clear, &QPushButton::clicked, this, [this]() {
        const int r = m_table->currentRow();
        if (r >= 0 && m_core.setCommandShortcuts(m_ids.at(r), QStringList(), false)) {
            refreshRow(r);
        }
    });
    connect(reset, &QPushButton::clicked, this, [this]() {
        const int r = m_table->currentRow();
        if (r >= 0 && m_core.setCommandShortcuts(m_ids.at(r), QStringList(), true)) {
            refreshRow(r);
        }
    });
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
}

void ShortcutsDialog::refreshRow(int row) {
    QStringList shown;
    const QStringList keys = m_core.commandShortcuts(m_ids.at(row));
    for (const QString &key : keys) {
        shown.append(QKeySequence::fromString(key, QKeySequence::PortableText)
                         .toString(QKeySequence::NativeText));
    }
    m_table->setItem(row, 2, new QTableWidgetItem(shown.join(QStringLiteral(", "))));
}

void ShortcutsDialog::editRow(int row) {
    if (row < 0 || row >= m_ids.size()) {
        return;
    }
    QDialog ask(this);
    ask.setWindowTitle(tr("Change Shortcut"));
    auto *edit = new QKeySequenceEdit(&ask);
    edit->setMaximumSequenceLength(1);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &ask);
    auto *layout = new QVBoxLayout(&ask);
    layout->addWidget(new QLabel(tr("Press the new shortcut:"), &ask));
    layout->addWidget(edit);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &ask, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &ask, &QDialog::reject);
    if (ask.exec() != QDialog::Accepted || edit->keySequence().isEmpty()) {
        return;
    }
    const QString key = edit->keySequence().toString(QKeySequence::PortableText);
    const QStringList others = m_core.shortcutConflicts(m_ids.at(row), key);
    if (!others.isEmpty()) {
        const auto answer = QMessageBox::question(this, tr("Shortcut in Use"),
                                                  tr("%1 is also used by %2. Assign it anyway?")
                                                      .arg(key, others.join(QStringLiteral(", "))));
        if (answer != QMessageBox::Yes) {
            return;
        }
    }
    if (!m_core.setCommandShortcuts(m_ids.at(row), {key}, false)) {
        QMessageBox::warning(this, tr("Invalid Shortcut"), tr("That shortcut cannot be used."));
        return;
    }
    refreshRow(row);
}

}  // namespace ferny
