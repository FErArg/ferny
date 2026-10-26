// The Files sidebar: a lazily expanded directory tree. Listings come from
// the core, which bounds them, never enters linked directories and escapes
// deceptive characters in names (threats T6, T7).
#pragma once

#include <QHash>
#include <QString>
#include <QStringList>
#include <QWidget>

#include "ferny_bridge/src/core_object.cxxqt.h"

class QLineEdit;
class QTreeWidget;
class QTreeWidgetItem;

namespace ferny {

class FileBrowser : public QWidget {
    Q_OBJECT

public:
    explicit FileBrowser(FernyCore &core, QWidget *parent = nullptr);

    // Shows `dir` (canonicalised; empty = home). False if it cannot be
    // listed; the tree then says so.
    bool setRoot(const QString &dir);
    QString root() const { return m_root; }

    // Whether dot files are listed (off by default, as in Geany).
    void setShowHidden(bool show);

    // Names shown at the top level, in order (smoke tests).
    QStringList topLevelNames() const;

    // Git status (H4.2d): letter by absolute path (`M`, `A`, `D`, `R`, `U`,
    // `?`); folders holding a change show `•`. Empty clears it.
    void setGitStatus(const QHash<QString, QString> &status);
    // "name=letter" for marked top-level entries (smoke tests).
    QStringList topLevelStatus() const;

signals:
    // The shown folder changed (Git status is asked for again).
    void rootChanged(const QString &root);
    // A file was activated (double-click or Enter).
    void fileActivated(const QString &path);
    // The user asked for the current document's folder.
    void currentFolderRequested();

private:
    // Lists `dir` under `parent` (nullptr = top level). False on failure.
    bool populate(QTreeWidgetItem *parent, const QString &dir);
    void activate(QTreeWidgetItem *item);
    void applyGit(QTreeWidgetItem *item) const;

    FernyCore &m_core;
    QLineEdit *m_path;
    QTreeWidget *m_tree;
    QString m_root;
    bool m_showHidden = false;
    QHash<QString, QString> m_git;
};

}  // namespace ferny
