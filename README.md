# Rust1090
<img src="https://waka.hackclub.com/api/badge/U078H10Q3MZ/interval:any/project:ADSB%20flight%20info">

A (worse) parody of dump1090

# Demo
https://plane.thijmens.nl for plane tracking<br>
https://plane.thijmens.nl/stats for statistics

# What is this?
This is a program that I wrote to run alongside of dump1090. Dump1090 is a program used to decode messages directly from aircraft, which includes data such as: altitude, coordinates and speed. The reason its made to run alongside dump1090 rather than run standalone is because I haven't figured out yet how to receive data directly from the antenne.

With this program I want to add some addition features to dump1090 including: statistics tracking, <a href="https://plane.thijmens.nl/stats">a dashboard</a> and last but not least: <a href="https://plane.thijmens.nl/">dark mode</a>.

# Why it took me so long
A weird segment to put into the README but I just wanted to clear this up. As the wakatime badge shows, I spent roughly working 31 hours on this project. This is a long time, compared to the amount of lines in the source code, especially because its heavily inspired by dump1090. The explanation is simple; I have tried a lot of different ways to get the result I wanted, which was decoding the messages myself. However, after <a href="https://github.com/TuinboonDev/temp1090/branches/all?query=idea">3 attempts</a> which all failed because the data was always slightly off. I decided to cut corners and use dump1090 to get the decoded data and build on top of that.

# How to use
The easiest way to use this is by going to the <a href="https://plane.thijmens.nl/stats">demo</a>.
However, if you find this really cool, you could pick up a 1090MHz antenne like <a href="https://www.amazon.nl/-/en/Magnetic-Receiver-Aviation-Definition-Software/dp/B07ZH5FJBW">this one</a> and an SDR dongle like <a href="https://flightaware.store/products/pro-stick-plus">this one</a>

If you still want to proceed ... xD
You can git clone:
```
git clone https://github.com/TuinboonDev/Rust1090
```
Setup an instance of <a href="https://couchdb.apache.org/">CouchDB</a><br>
Copy over an fill in the <a href="">`.env`</a> file and lastly:
```
cargo run
```

Yeah I know, not the most easy thing to use yourself.