// Single instance (threat T8, decision R3): a second invocation forwards
// its files to the running instance through a per-user local endpoint and
// exits. The only request is "open these files"; the core validates it.
#pragma once

#include <QObject>
#include <QString>

#include "ferny_bridge/src/core_object.cxxqt.h"

class QLocalServer;
class QLocalSocket;

namespace ferny {

class SingleInstance : public QObject {
    Q_OBJECT

public:
    enum class Role {
        Primary,     // listening; other invocations forward to this one
        Forwarded,   // the files were handed to the running instance
        Standalone,  // no single-instance support (endpoint unusable)
    };

    // `directory` is the private directory for the endpoint (Unix socket);
    // on Windows a per-user pipe name is used instead.
    SingleInstance(FernyCore &core, const QString &directory, QObject *parent = nullptr);

    // Forwards the core's pending files to a running instance or, if none
    // answers, starts listening.
    Role start();

Q_SIGNALS:
    // A validated request replaced the core's pending files.
    void openRequested();

private:
    void acceptConnection();
    void readRequest(QLocalSocket *socket);
    bool forward();

    FernyCore &m_core;
    QString m_directory;
    QString m_name;
    QLocalServer *m_server = nullptr;
};

}  // namespace ferny
