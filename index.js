console.log("Querying API search");

fetch("/api/search", {
    method: "POST",
    mode: "cors", 
    cache: "no-cache", 
    credentials: "same-origin", 
    headers: {
        "Content-Type": "text/plain",
    },
    body: "bind texture, to buffer.", 
}).then((response) => console.log(response));