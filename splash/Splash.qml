import QtQuick

Rectangle {
    id: root
    color: "#0c0e12"
    property int stage   // ksplash sets 1..6 as the session comes up

    // grain frames pre-rendered by splashframes.py: r00..r19 reveal, l00..l11 loop, ready
    property int tick: 0
    readonly property bool ready: stage >= 6
    readonly property string frame: ready ? "ready"
        : tick < 20 ? "r" + String(tick).padStart(2, "0")
        : "l" + String((tick - 20) % 12).padStart(2, "0")

    Timer {
        interval: 70; repeat: true; running: !root.ready   // ~14 fps film grain
        onTriggered: root.tick++
    }

    Image {
        id: mark
        anchors.centerIn: parent
        anchors.verticalCenterOffset: -20
        source: "images/" + root.frame + ".png"
        smooth: false        // keep grain pixels sharp
        cache: true
        asynchronous: false
    }

    Rectangle {
        anchors.top: mark.bottom; anchors.topMargin: 8
        anchors.horizontalCenter: parent.horizontalCenter
        width: 180; height: 3; radius: 2
        color: "#1d2027"
        Rectangle {
            height: parent.height; radius: 2
            color: "#3b82f6"
            width: parent.width * Math.min(root.stage, 6) / 6
            Behavior on width { NumberAnimation { duration: 450; easing.type: Easing.OutCubic } }
        }
    }

    OpacityAnimator { id: outro; target: root; from: 1; to: 0; duration: 500 }
    onReadyChanged: if (ready) outro.start()
}
