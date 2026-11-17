import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.private.mpris as Mpris
import org.kde.plasma.private.battery as Bat
import org.kde.plasma.private.volume as Vol
import org.kde.notificationmanager as NotificationManager
import org.kde.plasma.plasma5support as P5Support

PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground

    // ignore the burst of history/initial values Plasma replays at startup
    property bool ready: false
    Timer { interval: 4000; running: true; onTriggered: root.ready = true }

    // ---------- media ----------
    Mpris.Mpris2Model { id: mpris }
    readonly property var player: mpris.currentPlayer
    readonly property bool hasMedia: !!player && !!player.track
    readonly property bool playing: hasMedia && player.playbackStatus === Mpris.PlaybackStatus.Playing

    // ---------- battery: charge flash + low-battery warning ----------
    Bat.BatteryControlModel { id: battery }
    property bool lowWarned: false
    Connections {
        target: battery
        function onPluggedInChanged() { if (root.ready) root.flash("charge", 3500); }
        function onPercentChanged() {
            if (battery.pluggedIn || battery.percent > 20) { root.lowWarned = false; return; }
            if (battery.percent <= 15 && !root.lowWarned && root.ready) { root.lowWarned = true; root.flash("low", 6000); }
        }
    }

    // ---------- volume ----------
    readonly property var sink: Vol.PreferredDevice.sink
    readonly property int volPct: sink ? Math.round(sink.volume / 65536 * 100) : 0   // 65536 = PulseAudio 100%
    readonly property bool volMuted: sink ? sink.muted : false
    onVolPctChanged: if (ready) flash("volume", 1600)
    onVolMutedChanged: if (ready) flash("volume", 1600)

    // ---------- notifications ----------
    NotificationManager.Settings { id: nset }
    readonly property bool dnd: nset.notificationsInhibitedUntil > new Date() || nset.notificationsInhibitedByApplication
    property string nTitle: ""
    property string nBody: ""
    property string nIcon: ""
    property string nApp: ""
    NotificationManager.Notifications {
        id: notifs
        showNotifications: true
        showJobs: false
        showExpired: true
        showDismissed: false
        sortMode: NotificationManager.Notifications.SortByDate
        sortOrder: Qt.DescendingOrder
        onRowsInserted: (parent, first, last) => {
            if (!root.ready || root.dnd) return;
            const i = notifs.index(first, 0);
            const created = notifs.data(i, NotificationManager.Notifications.CreatedRole);
            if (created && (Date.now() - created.getTime()) > 15000) return;   // old item resurfacing
            root.nTitle = notifs.data(i, NotificationManager.Notifications.SummaryRole) || "";
            root.nBody = notifs.data(i, NotificationManager.Notifications.BodyRole) || "";
            root.nApp = notifs.data(i, NotificationManager.Notifications.ApplicationNameRole) || "";
            root.nIcon = notifs.data(i, NotificationManager.Notifications.IconNameRole)
                      || notifs.data(i, NotificationManager.Notifications.ApplicationIconNameRole) || "notifications";
            root.nImage = notifs.data(i, NotificationManager.Notifications.ImageRole) || null;
            root.flash("notify", 5000);
            nCard.visible = true;
            cardTimer.restart();
        }
    }
    property var nImage: null
    property Item anchorItem: null   // the pill, set by the compact face

    // notification card: drops out of the island; plain glass from the Plasma style (rounded, translucent, blurred)
    Timer { id: cardTimer; interval: 5000; onTriggered: if (!cardHover.containsMouse) nCard.visible = false }
    PlasmaCore.Dialog {
        id: nCard
        visualParent: root.anchorItem
        location: PlasmaCore.Types.TopEdge
        type: PlasmaCore.Dialog.Notification
        flags: Qt.WindowDoesNotAcceptFocus | Qt.WindowStaysOnTopHint
        hideOnWindowDeactivate: false
        visible: false

        mainItem: Item {
            width: 380
            height: cardRow.implicitHeight + 8
            MouseArea {
                id: cardHover
                anchors.fill: parent
                hoverEnabled: true
                onExited: cardTimer.restart()
                onClicked: nCard.visible = false
            }
            RowLayout {
                id: cardRow
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: 4 }
                spacing: 14
                Kirigami.Icon {
                    Layout.preferredWidth: 44; Layout.preferredHeight: 44
                    Layout.alignment: Qt.AlignTop
                    source: root.nImage ? root.nImage : root.nIcon
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        Layout.fillWidth: true; elide: Text.ElideRight
                        text: root.nApp
                        color: "#9aa1ae"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 12
                        visible: text.length > 0
                    }
                    Text {
                        Layout.fillWidth: true; elide: Text.ElideRight
                        text: root.nTitle
                        color: "#ffffff"; font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: 16
                    }
                    Text {
                        Layout.fillWidth: true
                        text: root.nBody.replace(/<[^>]*>/g, "")
                        visible: text.length > 0
                        wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight
                        color: "#d6dae2"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 13
                    }
                }
            }
        }
    }

    // ---------- microphone in use (privacy dot) ----------
    property bool micActive: false
    P5Support.DataSource {
        id: sh; engine: "executable"
        onNewData: (source, data) => { root.micActive = parseInt(data.stdout) > 0; disconnectSource(source); }
    }
    Timer {
        interval: 3000; running: true; repeat: true; triggeredOnStart: true
        // count recording streams, skipping Plasma's own level meters
        onTriggered: sh.connectSource("pactl list source-outputs 2>/dev/null | grep 'application.name = \"' | grep -v -c -i -E 'plasmashell|pavucontrol' #" + Date.now())
    }

    // ---------- face selection ----------
    property string flashMode: ""
    function flash(m, ms) { flashMode = m; flashTimer.interval = ms; flashTimer.restart(); }
    Timer { id: flashTimer; onTriggered: root.flashMode = "" }
    readonly property string mode: flashMode !== "" ? flashMode : (hasMedia ? "media" : "idle")

    compactRepresentation: Item {
        id: compact
        readonly property real h: Math.max(16, height + 6)   // bleeds into the bar padding (3 px each side); every face scales from h
        Layout.fillHeight: true
        Layout.minimumHeight: 16
        readonly property real targetW: ({
            idle: h * 3.2, media: h * 10.5, charge: h * 7, low: h * 7.5,
            volume: h * 7.5, notify: h * 13
        })[root.mode] || h * 3.2
        Layout.minimumWidth: pill.width + 4
        Layout.preferredWidth: pill.width + 4

        Component.onCompleted: root.anchorItem = pill
        Rectangle {
            id: pill
            anchors.centerIn: parent
            height: compact.h
            width: compact.targetW + (hover.containsMouse ? compact.h * 0.6 : 0)
            radius: height / 2
            color: "#000000"
            border.color: root.mode === "low" ? "#7f1d1d" : "#1f232b"; border.width: 1
            clip: true
            Behavior on width { NumberAnimation { duration: 420; easing.type: Easing.OutBack; easing.overshoot: 1.1 } }

            component Face: Item {
                property string face
                anchors.fill: parent
                opacity: root.mode === face ? 1 : 0
                visible: opacity > 0
                Behavior on opacity { NumberAnimation { duration: 200 } }
            }

            Face {   // idle: soft blue dot
                face: "idle"
                Rectangle { anchors.centerIn: parent; width: pill.height * 0.28; height: width; radius: width / 2; color: "#3b82f6" }
            }

            Face {   // media
                face: "media"
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: pill.height * 0.18; anchors.rightMargin: pill.height * 0.45
                    spacing: pill.height * 0.3
                    Rectangle {
                        Layout.preferredWidth: pill.height * 0.72; Layout.preferredHeight: width
                        radius: width / 2; color: "#22262e"; clip: true
                        Image { anchors.fill: parent; source: root.player ? root.player.artUrl : ""; fillMode: Image.PreserveAspectCrop }
                    }
                    Text {
                        Layout.fillWidth: true; elide: Text.ElideRight
                        text: root.player ? root.player.track : ""
                        color: "#ffffff"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: pill.height * 0.5
                    }
                    Row {
                        spacing: pill.height * 0.08
                        height: pill.height * 0.55
                        Repeater {
                            model: 3
                            Rectangle {
                                anchors.bottom: parent.bottom
                                width: pill.height * 0.11; radius: width / 2; color: "#3b82f6"
                                height: pill.height * 0.25
                                SequentialAnimation on height {
                                    running: root.playing; loops: Animation.Infinite
                                    NumberAnimation { to: pill.height * (0.45 + 0.1 * index); duration: 260 + index * 70; easing.type: Easing.InOutSine }
                                    NumberAnimation { to: pill.height * 0.2; duration: 240 + index * 60; easing.type: Easing.InOutSine }
                                }
                            }
                        }
                    }
                }
            }

            Face {   // charge / low battery
                face: root.mode === "low" ? "low" : "charge"
                RowLayout {
                    anchors.centerIn: parent
                    spacing: pill.height * 0.25
                    Kirigami.Icon {
                        Layout.preferredWidth: pill.height * 0.8; Layout.preferredHeight: width
                        isMask: true
                        rotation: 90   // material battery is upright; turn terminal to the right
                        color: root.mode === "low" ? "#f87171" : (battery.pluggedIn ? "#34d399" : "#ffffff")
                        source: "battery-" + String(Math.round(battery.percent / 10) * 10).padStart(3, "0") + (battery.pluggedIn ? "-charging" : "")
                    }
                    Text {
                        text: root.mode === "low" ? "Low battery " + battery.percent + "%"
                              : (battery.pluggedIn ? "Charging " : "On battery ") + battery.percent + "%"
                        color: root.mode === "low" ? "#fca5a5" : "#ffffff"
                        font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: pill.height * 0.5
                    }
                }
            }

            Face {   // volume level
                face: "volume"
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: pill.height * 0.4; anchors.rightMargin: pill.height * 0.5
                    spacing: pill.height * 0.3
                    Kirigami.Icon {
                        Layout.preferredWidth: pill.height * 0.7; Layout.preferredHeight: width
                        isMask: true; color: "#ffffff"
                        source: root.volMuted || root.volPct === 0 ? "audio-volume-muted"
                              : root.volPct < 34 ? "audio-volume-low" : root.volPct < 67 ? "audio-volume-medium" : "audio-volume-high"
                    }
                    Rectangle {
                        Layout.fillWidth: true; height: pill.height * 0.22; radius: height / 2; color: "#22262e"
                        Rectangle {
                            height: parent.height; radius: height / 2
                            color: root.volMuted ? "#6b7383" : "#3b82f6"
                            width: Math.max(height, parent.width * Math.min(1, root.volPct / 100))
                            Behavior on width { NumberAnimation { duration: 120 } }
                        }
                    }
                    Text {
                        text: root.volMuted ? "Muted" : root.volPct + "%"
                        color: "#ffffff"; font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: pill.height * 0.45
                        font.features: { "tnum": 1 }
                    }
                }
            }

            Face {   // notification
                face: "notify"
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: pill.height * 0.2; anchors.rightMargin: pill.height * 0.5
                    spacing: pill.height * 0.3
                    Rectangle {
                        Layout.preferredWidth: pill.height * 0.72; Layout.preferredHeight: width
                        radius: width / 2; color: "#3b82f6"
                        Kirigami.Icon { anchors.centerIn: parent; width: parent.width * 0.62; height: width; source: root.nIcon }
                    }
                    Text {
                        Layout.fillWidth: true; elide: Text.ElideRight
                        textFormat: Text.StyledText
                        // summary in bold, body after it in grey
                        text: "<b>" + root.nTitle + "</b>" + (root.nBody ? "  <font color='#9aa1ae'>" + root.nBody.replace(/<[^>]*>/g, "") + "</font>" : "")
                        color: "#ffffff"; font.family: "Nunito"; font.pixelSize: pill.height * 0.48
                    }
                }
            }

            // privacy dot: an app is using the microphone
            Rectangle {
                anchors.right: parent.right; anchors.rightMargin: pill.height * 0.22
                anchors.verticalCenter: parent.verticalCenter
                width: pill.height * 0.24; height: width; radius: width / 2
                color: "#fb923c"
                visible: root.micActive
            }
        }

        MouseArea {
            id: hover
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.expanded = !root.expanded
        }
    }

    // ---------- expanded card ----------
    // mpris only pushes position on seeks; poll while visible so the progress bar moves
    Timer {
        interval: 1000; repeat: true
        running: root.expanded && root.playing
        onTriggered: if (root.player) root.player.updatePosition()
    }

    fullRepresentation: Item {
        Layout.preferredWidth: 360; Layout.preferredHeight: 170
        Layout.minimumWidth: 300; Layout.minimumHeight: 150

        RowLayout {
            anchors.fill: parent; anchors.margins: 12
            spacing: 14
            Rectangle {
                Layout.preferredWidth: parent.height; Layout.preferredHeight: parent.height
                radius: 20; color: "#22262e"; clip: true
                Image { anchors.fill: parent; source: root.player ? root.player.artUrl : ""; fillMode: Image.PreserveAspectCrop }
                Kirigami.Icon {
                    anchors.centerIn: parent; width: parent.width * 0.4; height: width
                    source: "media-default-album"; visible: !root.player || !root.player.artUrl; color: "#6b7383"
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                Text {
                    Layout.fillWidth: true; elide: Text.ElideRight
                    text: root.hasMedia ? root.player.track : "Nothing playing"
                    color: "#f4f5f7"; font.family: "Nunito"; font.weight: Font.Black; font.pixelSize: 17
                }
                Text {
                    Layout.fillWidth: true; elide: Text.ElideRight
                    text: root.hasMedia ? (root.player.artist || "") : (battery.percent + "% battery" + (battery.pluggedIn ? ", charging" : ""))
                    color: "#8b93a3"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 13
                }
                // song progress (mpris position and length are in microseconds)
                ColumnLayout {
                    Layout.fillWidth: true; Layout.topMargin: 6
                    spacing: 2
                    visible: root.hasMedia && root.player.length > 0
                    Rectangle {
                        Layout.fillWidth: true; height: 6; radius: 3; color: "#22262e"
                        Rectangle {
                            height: parent.height; radius: 3; color: "#3b82f6"
                            width: root.player && root.player.length > 0 ? parent.width * Math.min(1, root.player.position / root.player.length) : 0
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        function fmt(us) { const s = Math.floor(us / 1e6); return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0"); }
                        Text { text: root.player ? parent.fmt(root.player.position) : ""; color: "#8b93a3"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 11; font.features: { "tnum": 1 } }
                        Item { Layout.fillWidth: true }
                        Text { text: root.player ? parent.fmt(root.player.length) : ""; color: "#8b93a3"; font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 11; font.features: { "tnum": 1 } }
                    }
                }
                RowLayout {
                    Layout.topMargin: 4
                    spacing: 10
                    visible: root.hasMedia
                    Repeater {
                        model: [
                            { icon: "media-skip-backward", act: "prev", main: false },
                            { icon: root.playing ? "media-playback-pause" : "media-playback-start", act: "toggle", main: true },
                            { icon: "media-skip-forward", act: "next", main: false }
                        ]
                        Rectangle {
                            width: modelData.main ? 44 : 36; height: width; radius: width / 2
                            color: modelData.main ? "#3b82f6" : (b.containsMouse ? "#2a2f39" : "#22262e")
                            Kirigami.Icon { anchors.centerIn: parent; width: parent.width * 0.5; height: width; source: modelData.icon; isMask: true; color: "#ffffff" }
                            MouseArea {
                                id: b; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    if (modelData.act === "prev") root.player.Previous();
                                    else if (modelData.act === "next") root.player.Next();
                                    else root.player.PlayPause();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
