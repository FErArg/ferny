// Preferences and keyboard-shortcut dialogs. Both are built from data the
// core provides (setting descriptors, command registry); nothing about
// individual settings or commands is hard-coded here.
#pragma once

#include <QDialog>
#include <QHash>
#include <QString>
#include <QStringList>

#include "ferny_bridge/src/core_object.cxxqt.h"

class QTableWidget;
class QWidget;

namespace ferny {

class PreferencesDialog : public QDialog {
    Q_OBJECT

public:
    explicit PreferencesDialog(FernyCore &core, QWidget *parent = nullptr);

    // Writes the edited values into the core (validated there). Returns
    // the keys the core rejected.
    QStringList apply();

    // Test support (H5.2): the setting keys with an editor, and the
    // category titles in display order.
    QStringList editedKeys() const;
    QStringList pageTitles() const;

signals:
    // The General category links to the shortcuts editor; the window owns
    // its save/cancel behaviour.
    void editShortcutsRequested();

private:
    FernyCore &m_core;
    QHash<QString, QWidget *> m_editors;
    QStringList m_pageTitles;
};

class ShortcutsDialog : public QDialog {
    Q_OBJECT

public:
    explicit ShortcutsDialog(FernyCore &core, QWidget *parent = nullptr);

private:
    void editRow(int row);
    void refreshRow(int row);

    FernyCore &m_core;
    QTableWidget *m_table;
    QStringList m_ids;
};

}  // namespace ferny
