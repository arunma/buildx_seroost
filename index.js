console.log("Querying API search");

async function search(query) {

    const results = document.getElementById("results");
    results.innerHTML=""
    const response = await fetch("/api/search", {
        method: "POST",
        headers: {
            "Content-Type": "text/plain",
        },
        body: query,
    });


    const json = await response.json();
    console.log(json);

    for ([path, rank] of json) {
        let item = document.createElement("span");
        item.appendChild(document.createTextNode(path));
        item.appendChild(document.createElement("br"));

        results.appendChild(item);
    }


}

let query = document.getElementById("query");
let currentSearch = Promise.resolve();

query.addEventListener("keypress", (e) => {
    if (e.key == "Enter") {
        currentSearch.then(() => search(query.value));
    }
});
