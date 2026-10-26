#include "singleinstance.h"

#include <QByteArray>
#include <QDir>
#include <QLocalServer>
#include <QLocalSocket>
#include <QTimer>
#include <QtEndian>

#include <cstdio>

namespace ferny {

namespace {

// Time allowed to connect, to send a request and to receive one.
constexpr int kConnectMs = 500;
constexpr int kIoMs = 2000;

const QByteArray kOk = QByteArrayLiteral("ok\n");

QByteArray frame(const QByteArray &payload) {
    QByteArray framed(4, '\0');
    qToBigEndian(static_cast<quint32>(payload.size()), framed.data());
    return framed + payload;
}

// Logs an event without paths or contents (T10).
void logEvent(const char *event) {
    std::fprintf(stderr, "ferny: %s\n", event);
}

}  // namespace

SingleInstance::SingleInstance(FernyCore &core, const QString &directory, QObject *parent)
    : QObject(parent), m_core(core), m_directory(directory) {
#ifdef Q_OS_WIN
    // Named pipes live in one machine-wide namespace: include the user.
    QString user = qEnvironmentVariable("USERNAME");
    user.removeIf([](QChar c) { return !c.isLetterOrNumber(); });
    m_name = QStringLiteral("ferny-") + user;
#else
    m_name = QDir(directory).filePath(QStringLiteral("ferny.sock"));
#endif
}

SingleInstance::Role SingleInstance::start() {
    const EndpointStatus status = m_core.checkEndpoint(m_directory, m_name);
    if (status == EndpointStatus::Unusable || status == EndpointStatus::Foreign) {
        logEvent("single instance disabled: endpoint is not private to this user");
        return Role::Standalone;
    }
    if (status == EndpointStatus::Ours) {
        QLocalSocket probe;
        probe.connectToServer(m_name);
        if (probe.waitForConnected(kConnectMs)) {
            probe.disconnectFromServer();
            if (forward()) {
                return Role::Forwarded;
            }
            logEvent("single instance: running instance did not answer");
            return Role::Standalone;
        }
        // Nobody listens: a stale socket left by a crash.
        QLocalServer::removeServer(m_name);
    }
#ifdef Q_OS_WIN
    if (forward()) {
        return Role::Forwarded;
    }
#endif
    m_server = new QLocalServer(this);
    m_server->setSocketOptions(QLocalServer::UserAccessOption);
    if (!m_server->listen(m_name)) {
        logEvent("single instance disabled: cannot listen");
        delete m_server;
        m_server = nullptr;
        return Role::Standalone;
    }
    connect(m_server, &QLocalServer::newConnection, this, &SingleInstance::acceptConnection);
    return Role::Primary;
}

bool SingleInstance::forward() {
    const QByteArray request = m_core.encodeOpenRequest();
    if (request.isEmpty()) {
        return false;
    }
    QLocalSocket socket;
    socket.connectToServer(m_name);
    if (!socket.waitForConnected(kConnectMs)) {
        return false;
    }
    socket.write(frame(request));
    if (!socket.waitForBytesWritten(kIoMs) || !socket.waitForReadyRead(kIoMs)) {
        return false;
    }
    return socket.readAll() == kOk;
}

void SingleInstance::acceptConnection() {
    while (QLocalSocket *socket = m_server->nextPendingConnection()) {
        connect(socket, &QLocalSocket::disconnected, socket, &QObject::deleteLater);
        connect(socket, &QLocalSocket::readyRead, this, [this, socket]() { readRequest(socket); });
        // A client that stalls is dropped.
        QTimer::singleShot(kIoMs, socket, [socket]() { socket->abort(); });
    }
}

void SingleInstance::readRequest(QLocalSocket *socket) {
    const qint64 max = static_cast<qint64>(m_core.maxRequestBytes());
    if (socket->bytesAvailable() < 4) {
        return;
    }
    QByteArray header = socket->peek(4);
    const quint32 length = qFromBigEndian<quint32>(header.constData());
    if (length > max) {
        logEvent("single instance: request too large, dropped");
        socket->abort();
        return;
    }
    if (socket->bytesAvailable() < 4 + static_cast<qint64>(length)) {
        return;
    }
    socket->read(4);
    const QByteArray payload = socket->read(length);
    if (socket->bytesAvailable() > 0 || !m_core.acceptOpenRequest(payload)) {
        logEvent("single instance: invalid request, dropped");
        socket->abort();
        return;
    }
    socket->write(kOk);
    socket->flush();
    socket->disconnectFromServer();
    Q_EMIT openRequested();
}

}  // namespace ferny
