import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as P5Support

PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    // live state, refreshed from the system every 3 s and right after each click
    property bool wifi: false
    property bool bt: false
    property bool dnd: false
    property bool night: false
    property bool muted: false

    readonly property string stateCmd: "printf '%s|%s|%s|%s|%s' "
        + "\"$(nmcli radio wifi)\" "
        + "\"$(bluetoothctl show 2>/dev/null | awk '/Powered:/{print $2; exit}')\" "
        + "\"$(kreadconfig6 --file plasmanotifyrc --group DoNotDisturb --key Until)\" "
        + "\"$(kreadconfig6 --file kwinrc --group NightColor --key Active)\" "
        + "\"$(pactl get-sink-mute @DEFAULT_SINK@ | awk '{print $2}')\""

    P5Support.DataSource {
        id: sh
        engine: "executable"
        onNewData: (source, data) => {
            if (source.startsWith("printf")) {
                const p = (data.stdout || "").split("|");
                root.wifi = p[0] === "enabled";
                root.bt = p[1] === "yes";
                // Until holds a future date while Do Not Disturb is on
                root.dnd = !!p[2] && p[2].length > 0;
                root.night = p[3] === "true";
                root.muted = p[4] === "yes";
            }
            disconnectSource(source);
        }
        function run(cmd) { connectSource(cmd); }
    }
    function refresh() { sh.run(stateCmd); }
    Timer { interval: 3000; running: true; repeat: true; triggeredOnStart: true; onTriggered: root.refresh() }

    function toggle(key) {
        const reconf = " && gdbus call --session --dest org.kde.KWin --object-path /KWin --method org.kde.KWin.reconfigure";
        const cmds = {
            wifi: "nmcli radio wifi " + (wifi ? "off" : "on"),
            bt: "bluetoothctl power " + (bt ? "off" : "on"),
            dnd: dnd ? "kwriteconfig6 --notify --file plasmanotifyrc --group DoNotDisturb --key Until --delete"
                     : "kwriteconfig6 --notify --file plasmanotifyrc --group DoNotDisturb --key Until \"$(date -d '+50 years' '+%Y,%-m,%-d,%-H,%-M,%-S')\"",
            // on = constant night light until switched off
            night: night ? "kwriteconfig6 --file kwinrc --group NightColor --key Active false" + reconf
                         : "kwriteconfig6 --file kwinrc --group NightColor --key Active true && kwriteconfig6 --file kwinrc --group NightColor --key Mode Constant" + reconf,
            muted: "pactl set-sink-mute @DEFAULT_SINK@ toggle"
        };
        sh.run(cmds[key] + " ; true #" + Date.now());
        settle.restart();
    }
    Timer { id: settle; interval: 600; onTriggered: root.refresh() }

    fullRepresentation: Rectangle {
        Layout.minimumWidth: 300; Layout.minimumHeight: 110
        Layout.preferredWidth: 360; Layout.preferredHeight: 130
        radius: 28
        color: Qt.rgba(0.08, 0.09, 0.11, 0.86)
        border.color: "#262a33"; border.width: 1

        RowLayout {
            anchors.fill: parent; anchors.margins: 16
            spacing: 6
            Repeater {
                model: [
                    { key: "wifi", label: "Wi-Fi", icon: "network-wireless" },
                    { key: "bt", label: "Bluetooth", icon: "bluetooth-active" },
                    { key: "dnd", label: "Focus", icon: "notifications-disabled" },
                    { key: "night", label: "Night", icon: "redshift-status-on" },
                    { key: "muted", label: "Mute", icon: "audio-volume-muted" }
                ]
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    readonly property bool on: root[modelData.key]
                    Rectangle {
                        Layout.alignment: Qt.AlignHCenter
                        width: 52; height: 52; radius: 26
                        color: parent.on ? "#3b82f6" : (tap.containsMouse ? "#2a2f39" : "#22262e")
                        Behavior on color { ColorAnimation { duration: 160 } }
                        Kirigami.Icon {
                            anchors.centerIn: parent; width: 24; height: 24
                            source: modelData.icon; isMask: true
                            color: parent.parent.on ? "#ffffff" : "#c9ceda"
                        }
                        MouseArea {
                            id: tap
                            anchors.fill: parent; hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.toggle(modelData.key)
                        }
                    }
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: modelData.label
                        color: parent.on ? "#f4f5f7" : "#8b93a3"
                        font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 12
                    }
                }
            }
        }
    }
}
