import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.private.mpris as Mpris

PlasmoidItem {
    id: root
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    Mpris.Mpris2Model { id: mpris }
    readonly property var player: mpris.currentPlayer
    readonly property bool playing: player && player.playbackStatus === Mpris.PlaybackStatus.Playing

    fullRepresentation: Rectangle {
        Layout.minimumWidth: 300; Layout.minimumHeight: 110
        Layout.preferredWidth: 360; Layout.preferredHeight: 130
        radius: 28
        color: Qt.rgba(0.08, 0.09, 0.11, 0.86)
        border.color: "#262a33"; border.width: 1

        RowLayout {
            anchors.fill: parent; anchors.margins: 16
            spacing: 14

            Rectangle {   // album art, or a flat tile when there is none
                Layout.preferredWidth: parent.height; Layout.preferredHeight: parent.height
                radius: 18; color: "#22262e"; clip: true
                Image {
                    anchors.fill: parent
                    source: root.player ? root.player.artUrl : ""
                    fillMode: Image.PreserveAspectCrop
                    visible: status === Image.Ready
                }
                Kirigami.Icon {
                    anchors.centerIn: parent; width: parent.width * 0.42; height: width
                    source: "media-default-album"
                    visible: !root.player || !root.player.artUrl
                    color: "#6b7383"
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                Text {
                    Layout.fillWidth: true
                    text: root.player && root.player.track ? root.player.track : "Nothing playing"
                    elide: Text.ElideRight
                    color: "#f4f5f7"
                    font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 16
                }
                Text {
                    Layout.fillWidth: true
                    text: root.player ? (root.player.artist || "") : "Start music or a video"
                    elide: Text.ElideRight
                    color: "#8b93a3"
                    font.family: "Nunito"; font.weight: Font.Bold; font.pixelSize: 13
                }
                RowLayout {
                    spacing: 8
                    Layout.topMargin: 4
                    Repeater {
                        model: [
                            { icon: "media-skip-backward", act: "prev", main: false },
                            { icon: root.playing ? "media-playback-pause" : "media-playback-start", act: "toggle", main: true },
                            { icon: "media-skip-forward", act: "next", main: false }
                        ]
                        Rectangle {
                            width: modelData.main ? 40 : 34; height: width; radius: width / 2
                            color: modelData.main ? "#3b82f6" : (hover.containsMouse ? "#2a2f39" : "#22262e")
                            opacity: root.player ? 1 : 0.4
                            Kirigami.Icon {
                                anchors.centerIn: parent; width: parent.width * 0.48; height: width
                                source: modelData.icon; color: "#ffffff"; isMask: true
                            }
                            MouseArea {
                                id: hover
                                anchors.fill: parent; hoverEnabled: true
                                enabled: !!root.player
                                cursorShape: Qt.PointingHandCursor
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
