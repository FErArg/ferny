#include "filebrowser.h"

#include <QAction>
#include <QApplication>
#include <QDir>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLineEdit>
#include <QStyle>
#include <QToolButton>
#include <QTreeWidget>
#include <QVBoxLayout>

namespace ferny {

namespace {

// Item data roles.
constexpr int kPathRole = Qt::UserRole;
constexpr int kKindRole = Qt::UserRole + 1;
// A directory whose children have not been listed yet.
constexpr int kPendingRole = Qt::UserRole + 2;

QToolButton *toolButton(QWidget *parent, QStyle::StandardPixmap icon, const QString &tip) {
    auto *button = new QToolButton(parent);
    button->setIcon(parent->style()->standardIcon(icon));
    button->setToolTip(tip);
    button->setAutoRaise(true);
    return button;
}

}  // namespace

FileBrowser::FileBrowser(FernyCore &core, QWidget *parent)
    : QWidget(parent), m_core(core), m_path(new QLineEdit(this)), m_tree(new QTreeWidget(this)) {
    auto *up = toolButton(this, QStyle::SP_FileDialogToParent, tr("Up"));
    auto *home = toolButton(this, QStyle::SP_DirHomeIcon, tr("Home Folder"));
    auto *current =
        toolButton(this, QStyle::SP_FileDialogContentsView, tr("Folder of the Current Document"));
    auto *refresh = toolButton(this, QStyle::SP_BrowserReload, tr("Refresh"));
    auto *bar = new QHBoxLayout;
    bar->setContentsMargins(0, 0, 0, 0);
    bar->setSpacing(0);
    bar->addWidget(up);
    bar->addWidget(home);
    bar->addWidget(current);
    bar->addWidget(refresh);
    bar->addStretch(1);
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(2);
    layout->addLayout(bar);
    layout->addWidget(m_path);
    layout->addWidget(m_tree, 1);
    m_path->setPlaceholderText(tr("Folder"));
    m_tree->setHeaderHidden(true);
    // Column 1: Git status letters (H4.2d).
    m_tree->setColumnCount(2);
    m_tree->header()->setStretchLastSection(false);
    m_tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_tree->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
    m_tree->setUniformRowHeights(true);

    connect(up, &QToolButton::clicked, this, [this]() {
        QDir dir(m_root);
        if (dir.cdUp()) {
            setRoot(dir.absolutePath());
        }
    });
    connect(home, &QToolButton::clicked, this, [this]() { setRoot(QString()); });
    connect(current, &QToolButton::clicked, this, &FileBrowser::currentFolderRequested);
    connect(refresh, &QToolButton::clicked, this, [this]() { setRoot(m_root); });
    connect(m_path, &QLineEdit::returnPressed, this, [this]() { setRoot(m_path->text()); });
    connect(m_tree, &QTreeWidget::itemExpanded, this, [this](QTreeWidgetItem *item) {
        if (item->data(0, kPendingRole).toBool()) {
            item->setData(0, kPendingRole, false);
            qDeleteAll(item->takeChildren());
            populate(item, item->data(0, kPathRole).toString());
        }
    });
    connect(m_tree, &QTreeWidget::itemActivated, this, &FileBrowser::activate);
}

bool FileBrowser::setRoot(const QString &dir) {
    const QString wanted = dir.isEmpty() ? QDir::homePath() : dir;
    // The root is canonical: the core refuses to list through a link.
    const QString canonical = QFileInfo(wanted).canonicalFilePath();
    m_root = canonical.isEmpty() ? wanted : canonical;
    m_path->setText(m_root);
    m_tree->clear();
    m_git.clear();
    const bool listed = populate(nullptr, m_root);
    emit rootChanged(m_root);
    return listed;
}

void FileBrowser::applyGit(QTreeWidgetItem *item) const {
    const QString path = item->data(0, kPathRole).toString();
    QString letter = m_git.value(path);
    if (letter.isEmpty() && !path.isEmpty() &&
        static_cast<BrowseKind>(item->data(0, kKindRole).toInt()) == BrowseKind::Directory) {
        const QString prefix = path + QLatin1Char('/');
        for (auto it = m_git.cbegin(); it != m_git.cend(); ++it) {
            if (it.key().startsWith(prefix)) {
                letter = QStringLiteral("•");
                break;
            }
        }
    }
    item->setText(1, letter);
    const QString tip = letter == QStringLiteral("M")   ? tr("Modified")
                        : letter == QStringLiteral("A") ? tr("Added")
                        : letter == QStringLiteral("D") ? tr("Deleted")
                        : letter == QStringLiteral("R") ? tr("Renamed")
                        : letter == QStringLiteral("U") ? tr("Conflict")
                        : letter == QStringLiteral("?") ? tr("Not tracked")
                        : letter.isEmpty()              ? QString()
                                                        : tr("Contains changes");
    item->setToolTip(1, tip);
    for (int i = 0; i < item->childCount(); ++i) {
        applyGit(item->child(i));
    }
}

void FileBrowser::setGitStatus(const QHash<QString, QString> &status) {
    m_git = status;
    for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
        applyGit(m_tree->topLevelItem(i));
    }
}

QStringList FileBrowser::topLevelStatus() const {
    QStringList marks;
    for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
        const QTreeWidgetItem *item = m_tree->topLevelItem(i);
        if (!item->text(1).isEmpty()) {
            marks.append(item->text(0) + QLatin1Char('=') + item->text(1));
        }
    }
    return marks;
}

void FileBrowser::setShowHidden(bool show) {
    if (show != m_showHidden) {
        m_showHidden = show;
        if (!m_root.isEmpty()) {
            setRoot(m_root);
        }
    }
}

QStringList FileBrowser::topLevelNames() const {
    QStringList names;
    for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
        names.append(m_tree->topLevelItem(i)->text(0));
    }
    return names;
}

bool FileBrowser::populate(QTreeWidgetItem *parent, const QString &dir) {
    const int count = m_core.listDirectory(dir, m_showHidden);
    QList<QTreeWidgetItem *> items;
    if (count < 0) {
        auto *item = new QTreeWidgetItem({tr("(cannot list this folder)")});
        item->setFlags(Qt::NoItemFlags);
        items.append(item);
    }
    QStyle *style = this->style();
    for (int i = 0; i < count; ++i) {
        const BrowseKind kind = m_core.listingKind(i);
        auto *item = new QTreeWidgetItem({m_core.listingName(i)});
        const QString path = m_core.listingPath(i);
        item->setData(0, kPathRole, path);
        item->setData(0, kKindRole, static_cast<int>(kind));
        switch (kind) {
            case BrowseKind::Directory:
                item->setIcon(0, style->standardIcon(QStyle::SP_DirIcon));
                // Listed when first expanded.
                item->setChildIndicatorPolicy(QTreeWidgetItem::ShowIndicator);
                item->setData(0, kPendingRole, true);
                break;
            case BrowseKind::DirectoryLink:
                item->setIcon(0, style->standardIcon(QStyle::SP_DirLinkIcon));
                item->setToolTip(0, tr("Link to a folder (not followed)"));
                break;
            case BrowseKind::FileLink:
                item->setIcon(0, style->standardIcon(QStyle::SP_FileLinkIcon));
                item->setToolTip(0, tr("Link"));
                break;
            case BrowseKind::File:
            default:
                item->setIcon(0, style->standardIcon(QStyle::SP_FileIcon));
                break;
        }
        items.append(item);
    }
    const int omitted = m_core.listingOmitted();
    if (omitted > 0) {
        auto *item = new QTreeWidgetItem({tr("(%n more not shown)", nullptr, omitted)});
        item->setFlags(Qt::NoItemFlags);
        items.append(item);
    }
    if (parent) {
        parent->addChildren(items);
    } else {
        m_tree->addTopLevelItems(items);
    }
    for (QTreeWidgetItem *item : items) {
        applyGit(item);
    }
    return count >= 0;
}

void FileBrowser::activate(QTreeWidgetItem *item) {
    if (!item) {
        return;
    }
    const auto kind = static_cast<BrowseKind>(item->data(0, kKindRole).toInt());
    const QString path = item->data(0, kPathRole).toString();
    if (path.isEmpty()) {
        return;
    }
    if (kind == BrowseKind::File || kind == BrowseKind::FileLink) {
        emit fileActivated(path);
    } else if (kind == BrowseKind::Directory) {
        item->setExpanded(!item->isExpanded());
    }
}

}  // namespace ferny
