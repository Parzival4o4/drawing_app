# Multi user Drawing web application

This project was created as part of my studies.  
It allows multiple users to draw together in real time.  

While the core functionality is complete, some areas could benefit from further refinement.  
Since this was primarily developed for educational purposes, I don’t plan to continue polishing it further.



## **Prerequisites**
- Node.js **v18.19.0**
- npm **9.2.0**
- Docker **Docker version 28.0.2, build 0442a73**
- Docker Compose **Docker Compose version v2.34.0**
- tested on debian 12 
- sqlx **sqlx-cli 0.8.6** (for Blatt 5)


To check your installed versions, run:  

```sh
node -v
npm -v
docker -v
docker compose version 
sqlx -V
```

development and testing was done on fedora 42

### **Build and Start the Docker Container**
Run the following command to build and start the Docker container:

```sh
docker-compose up --build

or

docker compose up --build
```

I have noticed some problems some compatibility problems with the sqlite db when starting from different host operating systems, you may need to rebuild the db with ` sqlx database setup `

### **Access the Web Page**
Once the container is running, open a browser and go to:

- **[http://localhost:8080](http://localhost:8080)** (for the main page)


# **Running the Project without Docker**

Build the frontend stuff
```sh 
cd frontend
npm run build
cd ..
```

Install sqlx-cli needed to build the db
```sh
cargo install sqlx-cli --features sqlite
```

Build the DB (sqlite)
```sh
sqlx database setup 
```

Run the webserver
```sh
JWT_SECRET=your_secret_here cargo run
```

